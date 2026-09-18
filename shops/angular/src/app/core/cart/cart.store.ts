import { Injectable, computed, inject, signal } from '@angular/core';
import { firstValueFrom } from 'rxjs';

import { CartApi, type CartResponse } from '../../api';
import { environment } from '../../../environments/environment';
import { formatApiError } from '../http/api-error';
import { cartWsUrl, parseCartUpdatedMessage } from './cart-ws';

const CART_ID_KEY = 'rs.cartId';

/**
 * Browser cart session: persists cart id and keeps a live cart snapshot.
 * Subscribes to `cart.updated` WebSocket push when a cart is open.
 */
@Injectable({ providedIn: 'root' })
export class CartStore {
  private readonly cartApi = inject(CartApi);

  private readonly cartSignal = signal<CartResponse | null>(null);
  private readonly busySignal = signal(false);
  private readonly errorSignal = signal<string | null>(null);

  private socket: WebSocket | null = null;
  private socketCartId: string | null = null;

  readonly cart = this.cartSignal.asReadonly();
  readonly busy = this.busySignal.asReadonly();
  readonly error = this.errorSignal.asReadonly();
  readonly lineCount = computed(() => {
    const cart = this.cartSignal();
    if (!cart) {
      return 0;
    }
    return cart.lines.reduce((sum, line) => sum + line.quantity, 0);
  });

  /** Loads the stored cart, or creates one when missing. */
  async ensureCart(): Promise<CartResponse> {
    const existingId = readCartId();
    if (existingId) {
      try {
        const cart = await firstValueFrom(this.cartApi.getCart(existingId));
        if (cart.status === 'open') {
          this.applyCart(cart);
          this.errorSignal.set(null);
          return cart;
        }
      } catch {
        clearCartId();
      }
    }
    return this.createCart();
  }

  /** Refreshes the current cart from the API when an id is known. */
  async refresh(): Promise<void> {
    const id = this.cartSignal()?.id ?? readCartId();
    if (!id) {
      this.applyCart(null);
      return;
    }
    this.busySignal.set(true);
    try {
      const cart = await firstValueFrom(this.cartApi.getCart(id));
      this.applyCart(cart);
      this.errorSignal.set(null);
    } catch (err) {
      this.errorSignal.set(formatError(err));
      throw err;
    } finally {
      this.busySignal.set(false);
    }
  }

  async addLine(variantId: string, quantity: number): Promise<CartResponse> {
    this.busySignal.set(true);
    this.errorSignal.set(null);
    try {
      const cart = await this.ensureCart();
      const updated = await firstValueFrom(
        this.cartApi.addLine(cart.id, { variant_id: variantId, quantity }),
      );
      this.applyCart(updated);
      return updated;
    } catch (err) {
      this.errorSignal.set(formatError(err));
      throw err;
    } finally {
      this.busySignal.set(false);
    }
  }

  async updateLine(lineId: string, quantity: number): Promise<CartResponse> {
    const cart = this.cartSignal();
    if (!cart) {
      throw new Error('No cart');
    }
    this.busySignal.set(true);
    this.errorSignal.set(null);
    try {
      const updated = await firstValueFrom(this.cartApi.updateLine(cart.id, lineId, { quantity }));
      this.applyCart(updated);
      return updated;
    } catch (err) {
      this.errorSignal.set(formatError(err));
      throw err;
    } finally {
      this.busySignal.set(false);
    }
  }

  async removeLine(lineId: string): Promise<CartResponse> {
    const cart = this.cartSignal();
    if (!cart) {
      throw new Error('No cart');
    }
    this.busySignal.set(true);
    this.errorSignal.set(null);
    try {
      const updated = await firstValueFrom(this.cartApi.deleteLine(cart.id, lineId));
      this.applyCart(updated);
      return updated;
    } catch (err) {
      this.errorSignal.set(formatError(err));
      throw err;
    } finally {
      this.busySignal.set(false);
    }
  }

  /** Clears the local cart session after a successful checkout. */
  clearSession(): void {
    clearCartId();
    this.applyCart(null);
    this.errorSignal.set(null);
  }

  private async createCart(): Promise<CartResponse> {
    this.busySignal.set(true);
    try {
      const cart = await firstValueFrom(this.cartApi.createCart({ currency: 'EUR' }));
      this.applyCart(cart);
      this.errorSignal.set(null);
      return cart;
    } catch (err) {
      this.errorSignal.set(formatError(err));
      throw err;
    } finally {
      this.busySignal.set(false);
    }
  }

  private applyCart(cart: CartResponse | null): void {
    this.cartSignal.set(cart);
    if (cart) {
      writeCartId(cart.id);
    }
    this.syncSocket(cart);
  }

  private syncSocket(cart: CartResponse | null): void {
    if (!cart || cart.status !== 'open' || !cart.token) {
      this.closeSocket();
      return;
    }
    if (
      this.socketCartId === cart.id &&
      this.socket &&
      (this.socket.readyState === WebSocket.CONNECTING ||
        this.socket.readyState === WebSocket.OPEN)
    ) {
      return;
    }
    this.closeSocket();
    this.openSocket(cart);
  }

  private openSocket(cart: CartResponse): void {
    const url = cartWsUrl(environment.apiBaseUrl, cart.id, cart.token);
    const socket = new WebSocket(url);
    this.socket = socket;
    this.socketCartId = cart.id;
    socket.onmessage = (event) => {
      const next = parseCartUpdatedMessage(String(event.data));
      if (!next) {
        return;
      }
      this.cartSignal.set(next);
      writeCartId(next.id);
    };
    socket.onerror = () => {
      // HTTP cart path remains authoritative; ignore socket noise.
    };
    socket.onclose = () => {
      if (this.socket === socket) {
        this.socket = null;
        this.socketCartId = null;
      }
    };
  }

  private closeSocket(): void {
    if (this.socket) {
      this.socket.onmessage = null;
      this.socket.onerror = null;
      this.socket.onclose = null;
      this.socket.close();
      this.socket = null;
    }
    this.socketCartId = null;
  }
}

function readCartId(): string | null {
  try {
    return localStorage.getItem(CART_ID_KEY);
  } catch {
    return null;
  }
}

function writeCartId(id: string): void {
  try {
    localStorage.setItem(CART_ID_KEY, id);
  } catch {
    // Private mode / blocked storage: cart still works for the session.
  }
}

function clearCartId(): void {
  try {
    localStorage.removeItem(CART_ID_KEY);
  } catch {
    // ignore
  }
}

function formatError(err: unknown): string {
  return formatApiError(err, 'Cart request failed');
}

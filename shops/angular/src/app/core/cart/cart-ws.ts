import type { CartResponse } from '../../api';

/** Parsed `cart.updated` WebSocket payload (server snapshot wins). */
export interface CartUpdatedPayload {
  type: 'cart.updated';
  version: number;
  cart: CartResponse;
}

/**
 * Parses a cart socket text frame.
 * Returns the cart snapshot for `cart.updated`, otherwise `null`.
 */
export function parseCartUpdatedMessage(raw: string): CartResponse | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw) as unknown;
  } catch {
    return null;
  }
  if (!parsed || typeof parsed !== 'object') {
    return null;
  }
  const record = parsed as Record<string, unknown>;
  if (record['type'] !== 'cart.updated') {
    return null;
  }
  const cart = record['cart'];
  if (!cart || typeof cart !== 'object') {
    return null;
  }
  const id = (cart as Record<string, unknown>)['id'];
  const token = (cart as Record<string, unknown>)['token'];
  if (typeof id !== 'string' || typeof token !== 'string') {
    return null;
  }
  return cart as CartResponse;
}

/**
 * Builds `ws(s)://…/v1/carts/{id}/ws?token=` from an HTTP API base URL.
 * Relative bases (e.g. `/api`) resolve against `window.location.origin`.
 */
export function cartWsUrl(apiBaseUrl: string, cartId: string, token: string): string {
  const base = apiBaseUrl.replace(/\/$/, '');
  const httpBase =
    base.startsWith('http://') || base.startsWith('https://')
      ? base
      : `${globalThis.location.origin}${base.startsWith('/') ? '' : '/'}${base}`;
  const wsBase = httpBase.replace(/^http/, 'ws');
  return `${wsBase}/v1/carts/${encodeURIComponent(cartId)}/ws?token=${encodeURIComponent(token)}`;
}

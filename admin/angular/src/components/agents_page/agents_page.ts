import { Component, OnDestroy, effect, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import { firstValueFrom } from 'rxjs';

import {
  AdminAiApi,
  AdminProductsApi,
  AdminProvidersApi,
  AdminSandboxApi,
  ApiClient,
  type AiProviderCatalogItemDto,
  type AiProviderStatusDto,
  type AiToolDto,
  type SandboxAuditDto,
  type SandboxJobDto,
} from '@rustashop/admin-api';
import { AdminTokenStore, formatApiError } from '@rustashop/admin-core';
import {
  template as agentsPageTpl,
  styles as agentsPageStyles,
} from '@generated/agents_page.ng';

@Component({
  selector: 'rs-agents-page',
  imports: [RouterLink, FormsModule],
  template: agentsPageTpl,
  styles: agentsPageStyles,
})
export class AgentsPage implements OnDestroy {
  private readonly ai = inject(AdminAiApi);
  private readonly providersApi = inject(AdminProvidersApi);
  private readonly products = inject(AdminProductsApi);
  private readonly sandbox = inject(AdminSandboxApi);
  private readonly api = inject(ApiClient);
  private readonly tokens = inject(AdminTokenStore);
  private autoSocket: WebSocket | null = null;
  private autoPoll: ReturnType<typeof setInterval> | null = null;

  protected readonly tools = signal<AiToolDto[]>([]);
  protected readonly audit = signal<SandboxAuditDto[]>([]);
  protected readonly productNames = signal<string[]>([]);
  protected readonly providers = signal<AiProviderStatusDto[]>([]);
  protected readonly catalog = signal<AiProviderCatalogItemDto[]>([]);
  protected readonly defaultProviderId = signal<string | null>(null);
  protected readonly testProviderId = signal('local');
  protected readonly testMessage = signal<string | null>(null);
  protected readonly autoJob = signal<SandboxJobDto | null>(null);
  protected readonly autoLogs = signal<string[]>([]);
  protected readonly autoCartSummary = signal<string | null>(null);
  protected readonly busy = signal(false);
  protected readonly error = signal<string | null>(null);
  protected readonly hasToken = this.tokens.hasToken;

  constructor() {
    effect(() => {
      if (this.tokens.hasToken()) {
        void this.reloadAll();
      } else {
        this.tools.set([]);
        this.audit.set([]);
        this.productNames.set([]);
        this.providers.set([]);
        this.catalog.set([]);
        this.defaultProviderId.set(null);
        this.testMessage.set(null);
        this.autoJob.set(null);
        this.autoLogs.set([]);
        this.autoCartSummary.set(null);
        this.error.set(null);
        this.closeAutoSocket();
        this.stopAutoPoll();
      }
    });
  }

  ngOnDestroy(): void {
    this.closeAutoSocket();
    this.stopAutoPoll();
  }

  protected autoLogText(): string {
    return this.autoLogs().join('\n');
  }

  protected async reloadTools(): Promise<void> {
    const token = this.tokens.token();
    if (!token) {
      return;
    }
    this.busy.set(true);
    this.error.set(null);
    try {
      this.tools.set(await this.ai.listTools(token));
    } catch (err) {
      this.error.set(formatApiError(err, 'Failed to load AI tools.'));
      this.tools.set([]);
    } finally {
      this.busy.set(false);
    }
  }

  protected async reloadProviders(): Promise<void> {
    const token = this.tokens.token();
    if (!token) {
      return;
    }
    this.busy.set(true);
    this.error.set(null);
    try {
      const [status, catalog] = await Promise.all([
        this.providersApi.listStatus(token),
        this.providersApi.listCatalog(token),
      ]);
      this.providers.set(status.providers);
      this.defaultProviderId.set(status.default_provider_id ?? null);
      this.catalog.set(catalog.providers);
      if (status.default_provider_id) {
        this.testProviderId.set(status.default_provider_id);
      }
    } catch (err) {
      this.error.set(formatApiError(err, 'Failed to load model providers.'));
      this.providers.set([]);
      this.catalog.set([]);
    } finally {
      this.busy.set(false);
    }
  }

  protected async runProviderTest(): Promise<void> {
    const token = this.tokens.token();
    if (!token) {
      return;
    }
    this.busy.set(true);
    this.error.set(null);
    this.testMessage.set(null);
    try {
      const result = await this.providersApi.testProvider(token, this.testProviderId());
      this.testMessage.set(
        result.ok
          ? `OK: ${result.provider_id}`
          : `Failed: ${result.error ?? 'unknown error'}`,
      );
    } catch (err) {
      this.error.set(formatApiError(err, 'Provider test failed.'));
    } finally {
      this.busy.set(false);
    }
  }

  protected async reloadAudit(): Promise<void> {
    const token = this.tokens.token();
    if (!token) {
      return;
    }
    try {
      this.audit.set(await this.sandbox.listAudit(token));
    } catch (err) {
      this.error.set(formatApiError(err, 'Failed to load audit.'));
    }
  }

  protected async runListProducts(): Promise<void> {
    const token = this.tokens.token();
    if (!token) {
      return;
    }
    this.busy.set(true);
    this.error.set(null);
    try {
      const page = await this.products.list(token);
      this.productNames.set(page.items.map((p) => p.name));
    } catch (err) {
      this.error.set(formatApiError(err, 'Failed to list products.'));
      this.productNames.set([]);
    } finally {
      this.busy.set(false);
    }
  }

  protected async runAutonomousDemo(): Promise<void> {
    const token = this.tokens.token();
    if (!token) {
      return;
    }
    this.busy.set(true);
    this.error.set(null);
    this.autoLogs.set([]);
    this.autoCartSummary.set(null);
    this.autoJob.set(null);
    this.closeAutoSocket();
    this.stopAutoPoll();
    try {
      const page = await this.products.list(token);
      const product = page.items[0];
      if (!product) {
        throw new Error('No catalog products; seed the database first.');
      }
      const detail = await firstValueFrom(
        this.api.http.get<{ variants: { id: string }[] }>(
          this.api.url(`/v1/products/${product.id}`),
        ),
      );
      const variantId = detail.variants[0]?.id;
      if (!variantId) {
        throw new Error('Product has no variants.');
      }
      const cart = await firstValueFrom(
        this.api.http.post<{ id: string }>(this.api.url('/v1/carts'), { currency: 'EUR' }),
      );
      await firstValueFrom(
        this.api.http.post(this.api.url(`/v1/carts/${cart.id}/lines`), {
          variant_id: variantId,
          quantity: 2,
        }),
      );
      const job = await this.sandbox.createCartQuantityJob(
        token,
        cart.id,
        variantId,
        5,
        'set',
      );
      this.autoJob.set(job);
      this.autoLogs.update((rows) => [...rows, `created cart ${cart.id} qty=2 → propose set 5`]);
      this.openAutoSocket(job.id, token);
      this.startAutoPoll(job.id, token);
    } catch (err) {
      this.error.set(formatApiError(err, 'Failed to start autonomous demo.'));
    } finally {
      this.busy.set(false);
    }
  }

  protected async commitAutonomous(): Promise<void> {
    const token = this.tokens.token();
    const job = this.autoJob();
    if (!token || !job) {
      return;
    }
    this.busy.set(true);
    this.error.set(null);
    try {
      const result = await this.sandbox.commitJob(token, job.id);
      this.autoJob.set(result.job);
      const qty = result.cart.lines[0]?.quantity ?? '?';
      this.autoCartSummary.set(`Committed: cart ${result.cart.id} line qty=${qty}`);
      await this.reloadAudit();
    } catch (err) {
      this.error.set(formatApiError(err, 'Commit failed.'));
    } finally {
      this.busy.set(false);
    }
  }

  protected async discardAutonomous(): Promise<void> {
    const token = this.tokens.token();
    const job = this.autoJob();
    if (!token || !job) {
      return;
    }
    this.busy.set(true);
    this.error.set(null);
    try {
      const discarded = await this.sandbox.discardJob(token, job.id);
      this.autoJob.set(discarded);
      this.autoCartSummary.set('Discarded: cart left unchanged.');
      await this.reloadAudit();
    } catch (err) {
      this.error.set(formatApiError(err, 'Discard failed.'));
    } finally {
      this.busy.set(false);
    }
  }

  private openAutoSocket(jobId: string, token: string): void {
    const url = this.sandbox.jobWsUrl(jobId, token);
    const socket = new WebSocket(url);
    this.autoSocket = socket;
    socket.onmessage = (event) => {
      try {
        const payload = JSON.parse(String(event.data)) as {
          type?: string;
          message?: string;
          status?: string;
        };
        if (payload.type === 'job.log' && payload.message) {
          this.autoLogs.update((rows) => [...rows, payload.message!]);
        } else if (payload.type === 'job.proposal') {
          this.autoLogs.update((rows) => [...rows, 'proposal ready']);
          void this.refreshAutoJob(jobId, token);
        } else if (payload.type === 'job.finished') {
          this.autoLogs.update((rows) => [
            ...rows,
            `finished: ${payload.status ?? '?'}`,
          ]);
          void this.refreshAutoJob(jobId, token);
          void this.reloadAudit();
        }
      } catch {
        this.autoLogs.update((rows) => [...rows, String(event.data)]);
      }
    };
    socket.onerror = () => {
      this.autoLogs.update((rows) => [...rows, 'websocket error']);
    };
  }

  private startAutoPoll(jobId: string, token: string): void {
    this.autoPoll = setInterval(() => {
      void this.refreshAutoJob(jobId, token);
    }, 1500);
  }

  private async refreshAutoJob(jobId: string, token: string): Promise<void> {
    try {
      const job = await this.sandbox.getJob(token, jobId);
      this.autoJob.set(job);
      if (job.status !== 'running') {
        this.stopAutoPoll();
      }
    } catch {
      // keep last known job; WS/logs still useful
    }
  }

  private closeAutoSocket(): void {
    this.autoSocket?.close();
    this.autoSocket = null;
  }

  private stopAutoPoll(): void {
    if (this.autoPoll) {
      clearInterval(this.autoPoll);
      this.autoPoll = null;
    }
  }

  private async reloadAll(): Promise<void> {
    await Promise.all([this.reloadTools(), this.reloadAudit(), this.reloadProviders()]);
  }
}

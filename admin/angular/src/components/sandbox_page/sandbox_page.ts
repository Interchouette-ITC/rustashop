import { Component, OnDestroy, effect, inject, signal } from '@angular/core';

import {
  AdminSandboxApi,
  type SandboxAuditDto,
  type SandboxJobDto,
} from '@rustashop/admin-api';
import { AdminTokenStore, formatApiError } from '@rustashop/admin-core';
import {
  template as sandboxPageTpl,
  styles as sandboxPageStyles,
} from '@generated/sandbox_page.ng';

@Component({
  selector: 'rs-sandbox-page',
  template: sandboxPageTpl,
  styles: sandboxPageStyles,
})
export class SandboxPage implements OnDestroy {
  private readonly api = inject(AdminSandboxApi);
  private readonly tokens = inject(AdminTokenStore);
  private socket: WebSocket | null = null;
  private pollTimer: ReturnType<typeof setInterval> | null = null;

  protected readonly job = signal<SandboxJobDto | null>(null);
  protected readonly audit = signal<SandboxAuditDto[]>([]);
  protected readonly logs = signal<string[]>([]);
  protected readonly busy = signal(false);
  protected readonly error = signal<string | null>(null);
  protected readonly hasToken = this.tokens.hasToken;

  constructor() {
    effect(() => {
      if (this.tokens.hasToken()) {
        void this.reloadAudit();
      } else {
        this.audit.set([]);
        this.error.set(null);
      }
    });
  }

  ngOnDestroy(): void {
    this.closeSocket();
    this.stopPoll();
  }

  protected logText(): string {
    return this.logs().join('\n');
  }

  protected async reloadAudit(): Promise<void> {
    const token = this.tokens.token();
    if (!token) {
      return;
    }
    try {
      this.audit.set(await this.api.listAudit(token));
    } catch (err) {
      this.error.set(formatApiError(err, 'Failed to load audit.'));
    }
  }

  protected async runQuote(): Promise<void> {
    const token = this.tokens.token();
    if (!token) {
      return;
    }
    this.busy.set(true);
    this.error.set(null);
    this.logs.set([]);
    this.job.set(null);
    this.closeSocket();
    this.stopPoll();
    try {
      const created = await this.api.createQuoteJob(token, 'EUR', [
        { sku: 'HOODIE-M', quantity: 2, unit_price_minor: 5000 },
      ]);
      this.job.set(created);
      this.openSocket(created.id, token);
      this.startPoll(created.id, token);
    } catch (err) {
      this.error.set(formatApiError(err, 'Failed to start quote job.'));
    } finally {
      this.busy.set(false);
    }
  }

  private openSocket(jobId: string, token: string): void {
    const url = this.api.jobWsUrl(jobId, token);
    const socket = new WebSocket(url);
    this.socket = socket;
    socket.onmessage = (event) => {
      try {
        const payload = JSON.parse(String(event.data)) as {
          type?: string;
          message?: string;
          status?: string;
        };
        if (payload.type === 'job.log' && payload.message) {
          this.logs.update((rows) => [...rows, payload.message!]);
        } else if (payload.type === 'job.finished') {
          this.logs.update((rows) => [...rows, `finished: ${payload.status ?? '?'}`]);
          void this.refreshJob(jobId, token);
          void this.reloadAudit();
        }
      } catch {
        this.logs.update((rows) => [...rows, String(event.data)]);
      }
    };
    socket.onerror = () => {
      this.logs.update((rows) => [...rows, 'websocket error']);
    };
  }

  private startPoll(jobId: string, token: string): void {
    this.pollTimer = setInterval(() => {
      void this.refreshJob(jobId, token);
    }, 1500);
  }

  private async refreshJob(jobId: string, token: string): Promise<void> {
    try {
      const latest = await this.api.getJob(token, jobId);
      this.job.set(latest);
      if (latest.status !== 'running') {
        this.stopPoll();
        void this.reloadAudit();
      }
    } catch {
      /* keep polling until finished or UI destroyed */
    }
  }

  private closeSocket(): void {
    if (this.socket) {
      this.socket.close();
      this.socket = null;
    }
  }

  private stopPoll(): void {
    if (this.pollTimer) {
      clearInterval(this.pollTimer);
      this.pollTimer = null;
    }
  }
}

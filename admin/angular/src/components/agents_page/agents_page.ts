import { Component, effect, inject, signal } from '@angular/core';
import { RouterLink } from '@angular/router';

import {
  AdminAiApi,
  AdminProductsApi,
  AdminSandboxApi,
  type AiToolDto,
  type SandboxAuditDto,
} from '@rustashop/admin-api';
import { AdminTokenStore, formatApiError } from '@rustashop/admin-core';
import {
  template as agentsPageTpl,
  styles as agentsPageStyles,
} from '@generated/agents_page.ng';

@Component({
  selector: 'rs-agents-page',
  imports: [RouterLink],
  template: agentsPageTpl,
  styles: agentsPageStyles,
})
export class AgentsPage {
  private readonly ai = inject(AdminAiApi);
  private readonly products = inject(AdminProductsApi);
  private readonly sandbox = inject(AdminSandboxApi);
  private readonly tokens = inject(AdminTokenStore);

  protected readonly tools = signal<AiToolDto[]>([]);
  protected readonly audit = signal<SandboxAuditDto[]>([]);
  protected readonly productNames = signal<string[]>([]);
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
        this.error.set(null);
      }
    });
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

  private async reloadAll(): Promise<void> {
    await Promise.all([this.reloadTools(), this.reloadAudit()]);
  }
}

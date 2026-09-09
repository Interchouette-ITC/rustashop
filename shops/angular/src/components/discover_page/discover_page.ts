import { Component, OnInit, inject, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { firstValueFrom } from 'rxjs';

import { CatalogApi, ShopAiApi, type AiToolDto } from '@rustashop/shop-api';
import { formatApiError } from '@rustashop/shop-core';
import {
  template as discoverPageTpl,
  styles as discoverPageStyles,
} from '@generated/discover_page.ng';

export interface ProductHit {
  id: string;
  name: string;
}

@Component({
  selector: 'rs-discover-page',
  imports: [RouterLink],
  template: discoverPageTpl,
  styles: discoverPageStyles,
})
export class DiscoverPage implements OnInit {
  private readonly ai = inject(ShopAiApi);
  private readonly catalog = inject(CatalogApi);

  protected readonly tools = signal<AiToolDto[]>([]);
  protected readonly productHits = signal<ProductHit[]>([]);
  protected readonly busy = signal(false);
  protected readonly error = signal<string | null>(null);

  ngOnInit(): void {
    void this.reloadTools();
  }

  protected async reloadTools(): Promise<void> {
    this.busy.set(true);
    this.error.set(null);
    try {
      const tools = await this.ai.listTools();
      this.tools.set(tools);
    } catch (err: unknown) {
      this.error.set(formatApiError(err, 'Failed to load AI tools.'));
      this.tools.set([]);
    } finally {
      this.busy.set(false);
    }
  }

  protected async runListProducts(): Promise<void> {
    this.busy.set(true);
    this.error.set(null);
    try {
      const page = await firstValueFrom(this.catalog.listProducts());
      this.productHits.set(
        (page.items ?? []).map((product) => ({
          id: product.id,
          name: product.name,
        })),
      );
    } catch (err: unknown) {
      this.error.set(formatApiError(err, 'Failed to list products.'));
      this.productHits.set([]);
    } finally {
      this.busy.set(false);
    }
  }
}

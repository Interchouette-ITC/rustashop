import { Injectable, inject } from '@angular/core';
import { firstValueFrom } from 'rxjs';

import { ApiClient } from './api-client';
import { environment } from '../../environments/environment';

export interface SandboxJobLineDto {
  sku: string;
  quantity: number;
  unit_price_minor: number;
}

export interface SandboxAdjustmentDto {
  label: string;
  amount_minor: number;
  currency: string;
}

export interface SandboxProposalDto {
  event_type: string;
  cart_id: string;
  product_id: string;
  quantity: number;
  operator: string;
}

export interface SandboxJobDto {
  id: string;
  job_type: string;
  status: string;
  source_hash: string;
  adjustments?: SandboxAdjustmentDto[];
  proposal?: SandboxProposalDto;
  error?: string;
}

export interface CommitSandboxJobDto {
  job: SandboxJobDto;
  cart: {
    id: string;
    lines: { id: string; variant_id: string; quantity: number }[];
  };
}

export interface SandboxAuditDto {
  job_id: string;
  actor: string;
  job_type: string;
  source_hash: string;
  status: string;
  created_at_unix: number;
}

/** Admin sandbox job + audit API. */
@Injectable({ providedIn: 'root' })
export class AdminSandboxApi {
  private readonly api = inject(ApiClient);

  createQuoteJob(token: string, currency: string, lines: SandboxJobLineDto[]): Promise<SandboxJobDto> {
    return firstValueFrom(
      this.api.http.post<SandboxJobDto>(
        this.api.adminUrl('sandbox/jobs'),
        { job_type: 'quote', currency, lines },
        { headers: { Authorization: `Bearer ${token}` } },
      ),
    );
  }

  createCartQuantityJob(
    token: string,
    cartId: string,
    variantId: string,
    quantity: number,
    operator: 'up' | 'down' | 'set',
  ): Promise<SandboxJobDto> {
    return firstValueFrom(
      this.api.http.post<SandboxJobDto>(
        this.api.adminUrl('sandbox/jobs'),
        {
          job_type: 'cart_quantity',
          cart_id: cartId,
          variant_id: variantId,
          quantity,
          operator,
        },
        { headers: { Authorization: `Bearer ${token}` } },
      ),
    );
  }

  commitJob(token: string, id: string): Promise<CommitSandboxJobDto> {
    return firstValueFrom(
      this.api.http.post<CommitSandboxJobDto>(
        this.api.adminUrl(`sandbox/jobs/${id}/commit`),
        {},
        { headers: { Authorization: `Bearer ${token}` } },
      ),
    );
  }

  discardJob(token: string, id: string): Promise<SandboxJobDto> {
    return firstValueFrom(
      this.api.http.post<SandboxJobDto>(
        this.api.adminUrl(`sandbox/jobs/${id}/discard`),
        {},
        { headers: { Authorization: `Bearer ${token}` } },
      ),
    );
  }

  getJob(token: string, id: string): Promise<SandboxJobDto> {
    return firstValueFrom(
      this.api.http.get<SandboxJobDto>(this.api.adminUrl(`sandbox/jobs/${id}`), {
        headers: { Authorization: `Bearer ${token}` },
      }),
    );
  }

  listAudit(token: string): Promise<SandboxAuditDto[]> {
    return firstValueFrom(
      this.api.http.get<SandboxAuditDto[]>(this.api.adminUrl('sandbox/audit'), {
        headers: { Authorization: `Bearer ${token}` },
      }),
    );
  }

  /** Browser WebSocket URL for job logs (`?token=` = admin bearer). */
  jobWsUrl(jobId: string, token: string): string {
    const base = environment.apiBaseUrl.replace(/\/$/, '');
    const prefix = environment.adminApiPrefix;
    const httpBase =
      base.startsWith('http://') || base.startsWith('https://')
        ? base
        : `${window.location.origin}${base.startsWith('/') ? '' : '/'}${base}`;
    const wsBase = httpBase.replace(/^http/, 'ws');
    return `${wsBase}/v1/${prefix}/sandbox/jobs/${jobId}/ws?token=${encodeURIComponent(token)}`;
  }
}

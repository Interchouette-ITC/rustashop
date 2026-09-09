import { Routes } from '@angular/router';

import { AdminShell } from '../components/admin_shell/admin_shell';

export const routes: Routes = [
  {
    path: '',
    component: AdminShell,
    children: [
      {
        path: '',
        loadComponent: () =>
          import('../components/orders_page/orders_page').then((m) => m.OrdersPage),
      },
      {
        path: 'products',
        loadComponent: () =>
          import('../components/products_page/products_page').then((m) => m.ProductsPage),
      },
      {
        path: 'agents',
        loadComponent: () =>
          import('../components/agents_page/agents_page').then((m) => m.AgentsPage),
      },
      {
        path: 'sandbox',
        loadComponent: () =>
          import('../components/sandbox_page/sandbox_page').then((m) => m.SandboxPage),
      },
      { path: '**', redirectTo: '' },
    ],
  },
];

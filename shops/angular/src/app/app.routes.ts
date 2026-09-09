import { Routes } from '@angular/router';

import { ShopShell } from '../components/shop_shell/shop_shell';

export const routes: Routes = [
  {
    path: '',
    component: ShopShell,
    children: [
      {
        path: '',
        loadComponent: () =>
          import('../components/product_list/product_list').then((m) => m.ProductListPage),
      },
      {
        path: 'products/:id',
        loadComponent: () =>
          import('../components/product_detail/product_detail').then((m) => m.ProductDetailPage),
      },
      {
        path: 'cart',
        loadComponent: () => import('../components/cart_page/cart_page').then((m) => m.CartPage),
      },
      {
        path: 'checkout/:orderId',
        loadComponent: () =>
          import('../components/checkout_page/checkout_page').then((m) => m.CheckoutPage),
      },
      {
        path: 'discover',
        loadComponent: () =>
          import('../components/discover_page/discover_page').then((m) => m.DiscoverPage),
      },
      { path: '**', redirectTo: '' },
    ],
  },
];

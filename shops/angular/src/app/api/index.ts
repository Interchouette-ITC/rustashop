export type { components, operations, paths } from './schema';

export { ApiClient } from './api-client';
export { HealthApi } from './health.api';
export { CatalogApi } from './catalog.api';
export { CartApi } from './cart.api';
export { CheckoutApi } from './checkout.api';
export { ShopAiApi } from './shop-ai.api';
export type { AiToolDto } from './shop-ai.api';

export type {
  AddCartLineRequest,
  CartLineResponse,
  CartResponse,
  CheckoutRequest,
  CreateCartRequest,
  HealthResponse,
  MoneyResponse,
  OrderLineResponse,
  OrderResponse,
  ProductDetailResponse,
  ProductListResponse,
  ProductResponse,
  ProductVariantResponse,
  UpdateCartLineRequest,
} from './models';

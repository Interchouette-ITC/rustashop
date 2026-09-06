//! `SeaORM` entity models for the commerce schema.

#![allow(missing_docs, clippy::empty_enums)]
pub mod cart;
pub mod cart_line;
pub mod category;
pub mod commerce_order;
pub mod customer;
pub mod order_line;
pub mod product;
pub mod product_variant;

pub mod prelude {
    //! Re-export active entities for query builders.

    pub use super::cart::{ActiveModel as Cart, Entity as CartEntity, Model as CartModel};
    pub use super::cart_line::{
        ActiveModel as CartLine, Entity as CartLineEntity, Model as CartLineModel,
    };
    pub use super::category::{
        ActiveModel as Category, Entity as CategoryEntity, Model as CategoryModel,
    };
    pub use super::commerce_order::{
        ActiveModel as CommerceOrder, Entity as CommerceOrderEntity, Model as CommerceOrderModel,
    };
    pub use super::customer::{
        ActiveModel as Customer, Entity as CustomerEntity, Model as CustomerModel,
    };
    pub use super::order_line::{
        ActiveModel as OrderLine, Entity as OrderLineEntity, Model as OrderLineModel,
    };
    pub use super::product::{
        ActiveModel as Product, Entity as ProductEntity, Model as ProductModel,
    };
    pub use super::product_variant::{
        ActiveModel as ProductVariant, Entity as ProductVariantEntity, Model as ProductVariantModel,
    };
}

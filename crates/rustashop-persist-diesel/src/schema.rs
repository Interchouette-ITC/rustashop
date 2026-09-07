//! Diesel schema for the catalog spike (`product` only).

diesel::table! {
    /// Catalog product row (matches `SQLx` / `SeaORM` migrations).
    product (id) {
        /// Primary key.
        id -> Uuid,
        /// Optional category foreign key.
        category_id -> Nullable<Uuid>,
        /// URL slug.
        slug -> Text,
        /// Display name.
        name -> Text,
        /// Optional long description.
        description -> Nullable<Text>,
        /// Storefront visibility.
        enabled -> Bool,
    }
}

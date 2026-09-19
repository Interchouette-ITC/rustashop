//! GPUI window for rustashop ops (orders + catalog).

#![allow(clippy::unreadable_literal)]

use gpui::{
    div, prelude::*, px, rgb, size, App, Bounds, Context, SharedString, TitlebarOptions, Window,
    WindowBounds, WindowOptions,
};

use rustashop_ops_gpui::{
    next_order_status, AdminClient, Config, Order, ProductDetail, Tab, ORDER_STATUSES,
};

/// Root view state for the ops window.
pub struct OpsApp {
    client: AdminClient,
    tab: Tab,
    orders: Vec<Order>,
    catalog: Vec<ProductDetail>,
    status: SharedString,
    error: Option<SharedString>,
}

impl OpsApp {
    /// Loads initial data from the Commerce API.
    pub fn bootstrap(client: AdminClient) -> Self {
        let mut app = Self {
            client,
            tab: Tab::Orders,
            orders: Vec::new(),
            catalog: Vec::new(),
            status: "ready".into(),
            error: None,
        };
        app.reload_orders();
        app
    }

    fn reload_orders(&mut self) {
        match self.client.list_orders() {
            Ok(orders) => {
                self.orders = orders;
                self.status = format!("{} orders", self.orders.len()).into();
                self.error = None;
            }
            Err(e) => {
                self.error = Some(e.into());
                self.status = "orders failed".into();
            }
        }
    }

    fn reload_catalog(&mut self) {
        match self.client.sync_catalog() {
            Ok(catalog) => {
                self.catalog = catalog;
                self.status = format!("{} products synced", self.catalog.len()).into();
                self.error = None;
            }
            Err(e) => {
                self.error = Some(e.into());
                self.status = "catalog sync failed".into();
            }
        }
    }

    fn advance_order(&mut self, order_id: &str) {
        let Some(order) = self.orders.iter().find(|o| o.id == order_id) else {
            return;
        };
        let Some(next) = next_order_status(&order.state) else {
            self.status = format!("order {} is terminal ({})", order.number, order.state).into();
            return;
        };
        match self.client.patch_order_status(order_id, next) {
            Ok(updated) => {
                if let Some(slot) = self.orders.iter_mut().find(|o| o.id == order_id) {
                    *slot = updated;
                }
                self.status = format!("patched to {next}").into();
                self.error = None;
            }
            Err(e) => {
                self.error = Some(e.into());
                self.status = "patch failed".into();
            }
        }
    }

    fn cancel_order(&mut self, order_id: &str) {
        match self.client.patch_order_status(order_id, "cancelled") {
            Ok(updated) => {
                if let Some(slot) = self.orders.iter_mut().find(|o| o.id == order_id) {
                    *slot = updated;
                }
                self.status = "cancelled".into();
                self.error = None;
            }
            Err(e) => {
                self.error = Some(e.into());
                self.status = "cancel failed".into();
            }
        }
    }
}

impl Render for OpsApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let base = self.client.config().api_base.clone();
        let header = SharedString::from(format!("rustashop ops  |  {base}"));

        div()
            .flex()
            .flex_col()
            .gap_3()
            .bg(rgb(0x12141a))
            .text_color(rgb(0xe8eaef))
            .size_full()
            .p_4()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("Ops / logistics"),
                    )
                    .child(div().text_xs().text_color(rgb(0x8a90a0)).child(header)),
            )
            .child(tab_bar(self.tab, cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .items_center()
                    .child(div().text_sm().text_color(rgb(0xa0a8b8)).child(self.status.clone()))
                    .child(action_chip("refresh", "Refresh", cx.listener(|this, _, _, cx| {
                        match this.tab {
                            Tab::Orders => this.reload_orders(),
                            Tab::Catalog => this.reload_catalog(),
                        }
                        cx.notify();
                    }))),
            )
            .when_some(self.error.clone(), |el, err| {
                el.child(div().text_sm().text_color(rgb(0xe0a040)).child(err))
            })
            .child(match self.tab {
                Tab::Orders => orders_pane(&self.orders, cx).into_any_element(),
                Tab::Catalog => catalog_pane(&self.catalog).into_any_element(),
            })
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x6a7080))
                    .child(format!(
                        "Statuses: {}  |  stock mouvements API: not shipped (variant stock via product detail)",
                        ORDER_STATUSES.join(", ")
                    )),
            )
    }
}

fn tab_bar(active: Tab, cx: &Context<OpsApp>) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .gap_2()
        .child(tab_chip(
            "Orders",
            active == Tab::Orders,
            cx.listener(|this, _, _, cx| {
                this.tab = Tab::Orders;
                this.reload_orders();
                cx.notify();
            }),
        ))
        .child(tab_chip(
            "Catalog",
            active == Tab::Catalog,
            cx.listener(|this, _, _, cx| {
                this.tab = Tab::Catalog;
                this.reload_catalog();
                cx.notify();
            }),
        ))
}

fn tab_chip(
    label: &'static str,
    active: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let bg = if active { rgb(0x2a3348) } else { rgb(0x1a1e28) };
    div()
        .id(SharedString::from(label))
        .px_3()
        .py_1()
        .rounded_md()
        .bg(bg)
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x323a50)))
        .on_click(on_click)
        .child(label)
}

fn action_chip(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label = label.into();
    div()
        .id(id.into())
        .px_2()
        .py_1()
        .rounded_md()
        .bg(rgb(0x243018))
        .text_color(rgb(0xb8d890))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x2e3c20)))
        .on_click(on_click)
        .child(label)
}

fn orders_pane(orders: &[Order], cx: &Context<OpsApp>) -> impl IntoElement {
    let mut list = div().flex().flex_col().gap_2().flex_1();
    if orders.is_empty() {
        list = list.child(div().text_sm().text_color(rgb(0x8a90a0)).child("No orders"));
    }
    for order in orders {
        let id = order.id.clone();
        let id_cancel = order.id.clone();
        let can_advance = next_order_status(&order.state).is_some();
        let can_cancel = order.state != "cancelled";
        list = list.child(
            div()
                .flex()
                .flex_row()
                .gap_3()
                .items_center()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(rgb(0x1a1e28))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .gap_1()
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child(format!("{}  ·  {}", order.number, order.state)),
                        )
                        .child(div().text_xs().text_color(rgb(0x8a90a0)).child(format!(
                            "pay {}  |  total {}",
                            order.payment_status,
                            order.total.display()
                        ))),
                )
                .when(can_advance, |row| {
                    row.child(action_chip(
                        format!("advance-{id}"),
                        "Advance",
                        cx.listener(move |this, _, _, cx| {
                            this.advance_order(&id);
                            cx.notify();
                        }),
                    ))
                })
                .when(can_cancel, |row| {
                    row.child(action_chip(
                        format!("cancel-{id_cancel}"),
                        "Cancel",
                        cx.listener(move |this, _, _, cx| {
                            this.cancel_order(&id_cancel);
                            cx.notify();
                        }),
                    ))
                }),
        );
    }
    list
}

fn catalog_pane(catalog: &[ProductDetail]) -> impl IntoElement {
    let mut list = div().flex().flex_col().gap_2().flex_1();
    if catalog.is_empty() {
        list = list.child(
            div()
                .text_sm()
                .text_color(rgb(0x8a90a0))
                .child("No products (open Catalog tab to sync)"),
        );
    }
    for product in catalog {
        let stock: i32 = product.variants.iter().map(|v| v.stock_quantity).sum();
        let skus = product
            .variants
            .iter()
            .map(|v| format!("{} ({})", v.sku, v.stock_quantity))
            .collect::<Vec<_>>()
            .join(", ");
        let enabled = if product.enabled { "on" } else { "off" };
        list = list.child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(rgb(0x1a1e28))
                .child(
                    div()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(format!("{}  ·  enabled {enabled}", product.name)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x8a90a0))
                        .child(if skus.is_empty() {
                            format!("stock total {stock} (no variants loaded)")
                        } else {
                            format!("stock total {stock}  |  {skus}")
                        }),
                ),
        );
    }
    list
}

/// Opens the ops window and runs the GPUI event loop.
pub fn run(config: Config) -> anyhow::Result<()> {
    let client = AdminClient::new(config).map_err(anyhow::Error::msg)?;
    gpui::Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1100.0), px(720.0)), cx);
        let open = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("rustashop ops".into()),
                    appears_transparent: false,
                    traffic_light_position: None,
                }),
                focus: true,
                show: true,
                app_id: Some("net.interchouette.rustashop-ops".into()),
                window_min_size: Some(size(px(800.0), px(520.0))),
                ..Default::default()
            },
            move |_window, cx| cx.new(|_| OpsApp::bootstrap(client)),
        );
        match open {
            Ok(_) => cx.activate(true),
            Err(e) => eprintln!("rustashop-ops-gpui: failed to open window: {e:#}"),
        }
    });
    Ok(())
}

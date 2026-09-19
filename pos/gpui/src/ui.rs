//! GPUI window for rustashop POS.

#![allow(clippy::unreadable_literal)]

use std::path::{Path, PathBuf};

use gpui::{
    div, prelude::*, px, rgb, size, App, Bounds, Context, SharedString, TitlebarOptions, Window,
    WindowBounds, WindowOptions,
};

use rustashop_pos_gpui::{
    CatalogClient, Config, Journal, Sale, SellableSku, TicketRecord,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Sale,
    Journal,
}

/// Root view state for the POS window.
pub struct PosApp {
    catalog: CatalogClient,
    journal: Journal,
    tab: Tab,
    skus: Vec<SellableSku>,
    sale: Sale,
    status: SharedString,
    error: Option<SharedString>,
    last_ticket: Option<TicketRecord>,
}

impl PosApp {
    /// Loads journal and leaves catalog empty until sync.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal cannot be opened.
    pub fn bootstrap(config: Config, journal_path: impl AsRef<Path>) -> Result<Self, String> {
        let catalog = CatalogClient::new(config)?;
        let journal = Journal::open(journal_path).map_err(|e| e.to_string())?;
        Ok(Self {
            catalog,
            journal,
            tab: Tab::Sale,
            skus: Vec::new(),
            sale: Sale::default(),
            status: "sync catalog to sell".into(),
            error: None,
            last_ticket: None,
        })
    }

    fn sync_catalog(&mut self) {
        match self.catalog.sync_sellable() {
            Ok(skus) => {
                self.status = format!("{} sellable SKUs", skus.len()).into();
                self.skus = skus;
                self.error = None;
            }
            Err(e) => {
                self.error = Some(e.into());
                self.status = "catalog sync failed".into();
            }
        }
    }

    fn pay_exact(&mut self) {
        if self.sale.is_empty() {
            self.status = "sale is empty".into();
            return;
        }
        let tendered = self.sale.total_minor();
        match self
            .journal
            .append_ticket(self.sale.to_ticket_lines(), tendered)
        {
            Ok(ticket) => {
                self.status = format!(
                    "ticket {} · {}",
                    ticket.id,
                    rustashop_pos_gpui::Money {
                        amount_minor: ticket.total_minor,
                        currency: ticket.currency.clone(),
                    }
                    .display()
                )
                .into();
                self.last_ticket = Some(ticket);
                self.sale.clear();
                self.error = None;
            }
            Err(e) => {
                self.error = Some(e.to_string().into());
                self.status = "ticket failed".into();
            }
        }
    }

    fn close_period(&mut self) {
        match self.journal.append_closure() {
            Ok(closure) => {
                self.status = format!(
                    "period close {} · {} tickets · {}",
                    closure.id,
                    closure.ticket_count,
                    rustashop_pos_gpui::Money {
                        amount_minor: closure.total_minor,
                        currency: closure.currency.clone(),
                    }
                    .display()
                )
                .into();
                self.error = None;
            }
            Err(e) => {
                self.error = Some(e.to_string().into());
                self.status = "period close failed".into();
            }
        }
    }
}

impl Render for PosApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let base = self.catalog.config().api_base.clone();
        let journal_path = self.journal.path().display().to_string();
        let header = SharedString::from(format!("rustashop POS  |  {base}"));

        div()
            .flex()
            .flex_col()
            .gap_3()
            .bg(rgb(0x141218))
            .text_color(rgb(0xf0ece4))
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
                            .child("POS"),
                    )
                    .child(div().text_xs().text_color(rgb(0x9a9488)).child(header)),
            )
            .child(tab_bar(self.tab, cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .items_center()
                    .child(div().text_sm().text_color(rgb(0xb0a898)).child(self.status.clone()))
                    .child(action_chip(
                        "sync",
                        "Sync catalog",
                        cx.listener(|this, _, _, cx| {
                            this.sync_catalog();
                            cx.notify();
                        }),
                    ))
                    .child(action_chip(
                        "close",
                        "Period close",
                        cx.listener(|this, _, _, cx| {
                            this.close_period();
                            cx.notify();
                        }),
                    )),
            )
            .when_some(self.error.clone(), |el, err| {
                el.child(div().text_sm().text_color(rgb(0xe0a040)).child(err))
            })
            .child(match self.tab {
                Tab::Sale => sale_pane(&self.skus, &self.sale, self.last_ticket.as_ref(), cx)
                    .into_any_element(),
                Tab::Journal => journal_pane(&self.journal, &journal_path).into_any_element(),
            })
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x6a655c))
                    .child(
                        "Journal is an append-only hash-chain stub. Not NF525 / LNE certified.",
                    ),
            )
    }
}

fn tab_bar(active: Tab, cx: &Context<PosApp>) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .gap_2()
        .child(tab_chip(
            "Sale",
            active == Tab::Sale,
            cx.listener(|this, _, _, cx| {
                this.tab = Tab::Sale;
                cx.notify();
            }),
        ))
        .child(tab_chip(
            "Journal",
            active == Tab::Journal,
            cx.listener(|this, _, _, cx| {
                this.tab = Tab::Journal;
                cx.notify();
            }),
        ))
}

fn tab_chip(
    label: &'static str,
    active: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let bg = if active { rgb(0x3a3028) } else { rgb(0x1e1a16) };
    div()
        .id(SharedString::from(label))
        .px_3()
        .py_1()
        .rounded_md()
        .bg(bg)
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x4a4038)))
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
        .bg(rgb(0x2a3820))
        .text_color(rgb(0xc8d8a0))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x3a4830)))
        .on_click(on_click)
        .child(label)
}

fn sale_pane(
    skus: &[SellableSku],
    sale: &Sale,
    last_ticket: Option<&TicketRecord>,
    cx: &Context<PosApp>,
) -> impl IntoElement {
    let total = rustashop_pos_gpui::Money {
        amount_minor: sale.total_minor(),
        currency: sale.currency().to_owned(),
    };

    div()
        .flex()
        .flex_row()
        .gap_3()
        .flex_1()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .flex_1()
                .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Catalog"))
                .child(catalog_list(skus, cx)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .w(px(360.0))
                .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Sale"))
                .child(sale_lines(sale))
                .child(
                    div()
                        .text_lg()
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(format!("Total {}", total.display())),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(action_chip(
                            "pay",
                            "Pay exact",
                            cx.listener(|this, _, _, cx| {
                                this.pay_exact();
                                cx.notify();
                            }),
                        ))
                        .child(action_chip(
                            "clear",
                            "Clear",
                            cx.listener(|this, _, _, cx| {
                                this.sale.clear();
                                this.status = "sale cleared".into();
                                cx.notify();
                            }),
                        )),
                )
                .when_some(last_ticket.map(|t| t.id.clone()), |el, id| {
                    el.child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x9a9488))
                            .child(format!("Last ticket: {id}")),
                    )
                }),
        )
}

fn catalog_list(skus: &[SellableSku], cx: &Context<PosApp>) -> impl IntoElement {
    let mut list = div().flex().flex_col().gap_1().flex_1();
    if skus.is_empty() {
        list = list.child(
            div()
                .text_sm()
                .text_color(rgb(0x9a9488))
                .child("No SKUs (Sync catalog)"),
        );
    }
    for sku in skus {
        let add = sku.clone();
        let id = format!("add-{}", sku.variant_id);
        list = list.child(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .items_center()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(rgb(0x1e1a16))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(div().child(format!("{} · {}", sku.product_name, sku.sku)))
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x9a9488))
                                .child(format!(
                                    "{} · stock {}",
                                    sku.unit_price.display(),
                                    sku.stock_quantity
                                )),
                        ),
                )
                .child(action_chip(id, "+", cx.listener(move |this, _, _, cx| {
                    this.sale.add_sku(&add);
                    this.status = format!("added {}", add.sku).into();
                    cx.notify();
                }))),
        );
    }
    list
}

fn sale_lines(sale: &Sale) -> impl IntoElement {
    let mut list = div().flex().flex_col().gap_1().flex_1();
    if sale.is_empty() {
        list = list.child(
            div()
                .text_sm()
                .text_color(rgb(0x9a9488))
                .child("Empty sale"),
        );
    }
    for line in sale.lines() {
        list = list.child(
            div()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(rgb(0x1e1a16))
                .child(format!(
                    "{} × {} · {}",
                    line.quantity,
                    line.sku,
                    rustashop_pos_gpui::Money {
                        amount_minor: line.line_total_minor(),
                        currency: line.unit_price.currency.clone(),
                    }
                    .display()
                )),
        );
    }
    list
}

fn journal_pane(journal: &Journal, path: &str) -> impl IntoElement {
    let open = journal.open_tickets();
    let recent = journal.recent_tickets(12);
    let mut list = div().flex().flex_col().gap_2().flex_1();
    list = list.child(
        div()
            .text_sm()
            .text_color(rgb(0x9a9488))
            .child(format!(
                "{}  ·  {} open tickets  ·  {} entries",
                path,
                open.len(),
                journal.len()
            )),
    );
    if recent.is_empty() {
        list = list.child(div().text_sm().child("No tickets yet"));
    }
    for ticket in recent {
        list = list.child(
            div()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(rgb(0x1e1a16))
                .child(format!(
                    "{} · seq {} · {} · hash {}…",
                    ticket.id,
                    ticket.seq,
                    rustashop_pos_gpui::Money {
                        amount_minor: ticket.total_minor,
                        currency: ticket.currency.clone(),
                    }
                    .display(),
                    ticket.hash.chars().take(8).collect::<String>()
                )),
        );
    }
    list
}

/// Opens the POS window and runs the GPUI event loop.
pub fn run(config: Config, journal_path: PathBuf) -> anyhow::Result<()> {
    let app = PosApp::bootstrap(config, journal_path).map_err(anyhow::Error::msg)?;
    gpui::Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1180.0), px(740.0)), cx);
        let open = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("rustashop POS".into()),
                    appears_transparent: false,
                    traffic_light_position: None,
                }),
                focus: true,
                show: true,
                app_id: Some("net.interchouette.rustashop-pos".into()),
                window_min_size: Some(size(px(900.0), px(560.0))),
                ..Default::default()
            },
            move |_window, cx| cx.new(|_| app),
        );
        match open {
            Ok(_) => cx.activate(true),
            Err(e) => eprintln!("rustashop-pos-gpui: failed to open window: {e:#}"),
        }
    });
    Ok(())
}

mod home;
mod okx;
mod text;

pub use okx::AccountBalance;

use iced::widget::{Space, button, column, container, row, scrollable, text as iced_text};
use iced::{Background, Border, Color, Element, Length, Subscription, Task, time};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const DRAWER_HEIGHT: f32 = 160.0;
const MAX_VISIBLE_PRICES: usize = 3;
const HOME_REFRESH_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Deserialize)]
pub struct Layout {
    item: Vec<Drawer>,
}

#[derive(Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Drawer {
    Okx(okx::Okx),
    Text(text::Text),
    Home(home::Home),
}

#[derive(Debug, Clone)]
pub enum Message {
    Refresh,
    BalanceLoaded(u64, Result<Vec<AccountBalance>, String>),
    HomeRefresh,
    HomeLoaded(u64, Result<home::HomeContent, String>),
    CopyHomeCommand(String),
    RotatePrices,
}

pub struct Manager {
    items: Arc<[Drawer]>,
    balances: Option<Vec<AccountBalance>>,
    prices: Vec<(String, String)>,
    price_rotation_index: usize,
    last_balance_refresh: Option<String>,
    balance_error: Option<String>,
    balance_refresh_id: u64,
    home_content: Option<home::HomeContent>,
    home_error: Option<String>,
    home_refresh_id: u64,
}

impl Manager {
    pub fn new(items: Arc<[Drawer]>) -> Self {
        Self {
            items,
            balances: None,
            prices: Vec::new(),
            price_rotation_index: 0,
            last_balance_refresh: None,
            balance_error: None,
            balance_refresh_id: 0,
            home_content: None,
            home_error: None,
            home_refresh_id: 0,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Refresh => self.refresh_okx(),
            Message::BalanceLoaded(refresh_id, result) => {
                if refresh_id != self.balance_refresh_id {
                    return Task::none();
                }

                match result {
                    Ok(balances) => {
                        self.prices = collect_asset_prices(&balances);
                        self.balances = Some(balances);
                        self.last_balance_refresh =
                            Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
                        self.balance_error = None;
                    }
                    Err(error) => {
                        self.balance_error = Some(error);
                    }
                }
                Task::none()
            }
            Message::HomeRefresh => self.refresh_home(),
            Message::HomeLoaded(refresh_id, result) => {
                if refresh_id != self.home_refresh_id {
                    return Task::none();
                }

                match result {
                    Ok(content) => {
                        if self.home_content.as_ref() != Some(&content) {
                            self.home_content = Some(content);
                        }
                        self.home_error = None;
                    }
                    Err(error) => {
                        self.home_error = Some(error);
                    }
                }
                Task::none()
            }
            Message::CopyHomeCommand(command) => iced::clipboard::write(command),
            Message::RotatePrices => {
                if self.prices.len() > MAX_VISIBLE_PRICES {
                    self.price_rotation_index = (self.price_rotation_index + 1) % self.prices.len();
                }
                Task::none()
            }
        }
    }

    pub fn refresh(&mut self) -> Task<Message> {
        Task::batch([self.refresh_okx(), self.refresh_home()])
    }

    fn refresh_okx(&mut self) -> Task<Message> {
        let Some(okx) = self.items.iter().find_map(|drawer| match drawer {
            Drawer::Okx(okx) => Some(okx.clone()),
            Drawer::Text(_) | Drawer::Home(_) => None,
        }) else {
            return Task::none();
        };

        self.balance_refresh_id = self.balance_refresh_id.wrapping_add(1);
        let refresh_id = self.balance_refresh_id;
        Task::perform(okx.get_balance(), move |result| {
            Message::BalanceLoaded(refresh_id, result)
        })
    }

    fn refresh_home(&mut self) -> Task<Message> {
        let Some(home) = self.items.iter().find_map(|drawer| match drawer {
            Drawer::Home(home) => Some(home.clone()),
            Drawer::Okx(_) | Drawer::Text(_) => None,
        }) else {
            return Task::none();
        };

        self.home_refresh_id = self.home_refresh_id.wrapping_add(1);
        let refresh_id = self.home_refresh_id;
        Task::perform(home.fetch(), move |result| {
            Message::HomeLoaded(refresh_id, result)
        })
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![time::every(Duration::from_secs(5)).map(|_| Message::Refresh)];
        if self
            .items
            .iter()
            .any(|drawer| matches!(drawer, Drawer::Home(_)))
        {
            subscriptions.push(time::every(HOME_REFRESH_INTERVAL).map(|_| Message::HomeRefresh));
        }
        if self.prices.len() > MAX_VISIBLE_PRICES {
            subscriptions.push(time::every(Duration::from_secs(2)).map(|_| Message::RotatePrices));
        }
        Subscription::batch(subscriptions)
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut drawers = column![].spacing(12).padding(12);

        for drawer in self.items.iter() {
            let card = match drawer {
                Drawer::Okx(_) => {
                    let accounts = self.balances.as_deref().unwrap_or_default();
                    let total_value = match accounts.first() {
                        Some(account) => match account.total_eq.parse::<f64>() {
                            Ok(value) if value.is_finite() => format!("{value:.2} USD"),
                            Ok(_) => "总价值不可用".to_owned(),
                            Err(error) => format!("总价值解析失败: {error}"),
                        },
                        None => "正在获取数据".to_owned(),
                    };
                    let left = column![
                        iced_text("总价值").size(40),
                        iced_text(total_value).size(18),
                        Space::new().height(Length::Fill),
                    ]
                    .spacing(4)
                    .width(Length::Fill);
                    let mut prices = column![].spacing(12);
                    for (currency, price) in self.visible_prices() {
                        prices = prices.push(
                            column![
                                iced_text(format!("{currency} 价格")).size(12),
                                iced_text(price).size(14)
                            ]
                            .spacing(1),
                        );
                    }
                    if self.prices.is_empty() {
                        prices = prices.push(iced_text("暂无币价").size(14));
                    }
                    let timestamp = self.last_balance_refresh.as_deref().unwrap_or("等待更新");
                    let mut footer = column![iced_text(format!("时间戳  {timestamp}")).size(12)]
                        .width(Length::Fill);
                    if let Some(error) = &self.balance_error {
                        footer = footer.push(iced_text(error).size(10));
                    }
                    drawer_card(
                        column![row![left, prices].spacing(8).height(Length::Fill), footer]
                            .spacing(4)
                            .height(Length::Fill),
                    )
                }
                Drawer::Text(value) => {
                    drawer_card(container(iced_text(value.content()).size(24)).center(Length::Fill))
                }
                Drawer::Home(_) => {
                    home_card(self.home_content.as_ref(), self.home_error.as_deref())
                }
            };
            drawers = drawers.push(card);
        }

        scrollable(drawers).height(Length::Fill).into()
    }

    fn visible_prices(&self) -> Vec<&(String, String)> {
        let count = self.prices.len();
        if count == 0 {
            return Vec::new();
        }

        let visible_count = count.min(MAX_VISIBLE_PRICES);
        let start = if count > MAX_VISIBLE_PRICES {
            self.price_rotation_index % count
        } else {
            0
        };

        (0..visible_count)
            .map(|offset| &self.prices[(start + offset) % count])
            .collect()
    }
}

fn home_card<'a>(
    content: Option<&'a home::HomeContent>,
    error: Option<&'a str>,
) -> Element<'a, Message> {
    let title = content.map_or("Home", |content| content.title.as_str());
    let mut card = column![iced_text(title).size(20)].spacing(8);

    if let Some(error) = error {
        card = card.push(iced_text(error).size(12));
    }

    match content {
        Some(content) if content.lines.is_empty() => {
            card = card.push(iced_text("页面暂无文字内容").size(14));
        }
        Some(content) => {
            for command in &content.lines {
                card = card.push(home_command(command));
            }
        }
        None if error.is_none() => {
            card = card.push(iced_text("正在获取 Notion 页面").size(14));
        }
        None => {}
    }

    drawer_card_with_height(card, Length::Shrink)
}

fn home_command(command: &str) -> Element<'_, Message> {
    button(row![
        container(
            iced_text(command)
                .size(12)
                .wrapping(iced::widget::text::Wrapping::None)
                .width(Length::Fill),
        )
        .width(Length::Fill)
        .clip(true),
        iced_text("复制").size(12),
    ])
    .width(Length::Fill)
    .padding([6, 8])
    .style(|_, status| {
        let background = match status {
            button::Status::Hovered => Color::from_rgb8(64, 64, 64),
            button::Status::Pressed => Color::from_rgb8(48, 48, 48),
            _ => Color::from_rgb8(40, 40, 40),
        };
        button::Style {
            text_color: Color::WHITE,
            background: Some(Background::Color(background)),
            border: Border::default()
                .color(Color::from_rgb8(96, 96, 96))
                .width(1.0)
                .rounded(6.0),
            ..Default::default()
        }
    })
    .on_press(Message::CopyHomeCommand(command.to_owned()))
    .into()
}

fn drawer_card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    drawer_card_with_height(content, DRAWER_HEIGHT)
}

fn drawer_card_with_height<'a>(
    content: impl Into<Element<'a, Message>>,
    height: impl Into<Length>,
) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .height(height)
        .padding(16)
        .style(|_| container::Style {
            text_color: Some(Color::WHITE),
            background: Some(Background::Color(Color::from_rgb8(32, 32, 32))),
            border: Border::default()
                .color(Color::WHITE)
                .width(1.5)
                .rounded(24.0),
            ..Default::default()
        })
        .into()
}

fn collect_asset_prices(accounts: &[AccountBalance]) -> Vec<(String, String)> {
    let mut prices = BTreeMap::new();

    for balance in accounts.iter().flat_map(|account| &account.details) {
        if balance.ccy.eq_ignore_ascii_case("USDT") {
            continue;
        }

        let price = match balance.spot_price_usd() {
            Ok(Some(price)) => format!("{price:.2} USD"),
            Ok(None) => continue,
            Err(error) => error,
        };
        prices.entry(balance.ccy.clone()).or_insert(price);
    }

    prices.into_iter().collect()
}

pub fn load() -> Result<Arc<[Drawer]>, Box<dyn Error>> {
    let path = std::env::current_exe()
        .ok()
        .and_then(|executable| bundled_config_path(&executable))
        .unwrap_or_else(|| Path::new("conf/layout.toml").to_path_buf());
    let contents = fs::read_to_string(&path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("failed to read {}: {error}", path.display()),
        )
    })?;
    let layout: Layout = toml::from_str(&contents).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse {}: {error}", path.display()),
        )
    })?;

    Ok(layout.item.into())
}

fn bundled_config_path(executable: &Path) -> Option<PathBuf> {
    let contents_dir = executable.parent()?.parent()?;
    let app_bundle_dir = contents_dir.parent()?;
    let is_app_bundle = contents_dir.file_name()? == "Contents"
        && app_bundle_dir
            .extension()
            .is_some_and(|extension| extension == "app");

    is_app_bundle.then(|| {
        contents_dir
            .join("Resources")
            .join("conf")
            .join("layout.toml")
    })
}

#[cfg(test)]
mod tests {
    use super::okx::BalanceDetail;
    use super::{
        AccountBalance, Drawer, Layout, Manager, Message, bundled_config_path,
        collect_asset_prices, load,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn parses_layout_items_into_drawers() {
        let layout: Layout = toml::from_str(
            r#"
                [[item]]
                type = "okx"
                apiKey = "api-key"
                secretKey = "secret-key"

                [[item]]
                type = "text"
                content = "hello"
            "#,
        )
        .expect("layout should parse");

        assert_eq!(layout.item.len(), 2);
        assert!(matches!(layout.item[0], Drawer::Okx(_)));
        assert!(matches!(layout.item[1], Drawer::Text(_)));
    }

    #[test]
    fn failed_balance_refresh_keeps_last_successful_result_and_time() {
        let mut manager = Manager::new(Vec::new().into());
        manager.balances = Some(vec![AccountBalance {
            total_eq: "123.45".to_owned(),
            details: Vec::new(),
        }]);
        manager.last_balance_refresh = Some("2026-11-25 13:00:00".to_owned());
        manager.balance_refresh_id = 1;

        let _task = manager.update(Message::BalanceLoaded(1, Err("request failed".to_owned())));

        assert_eq!(manager.balances.as_ref().unwrap()[0].total_eq, "123.45");
        assert_eq!(
            manager.last_balance_refresh.as_deref(),
            Some("2026-11-25 13:00:00")
        );
        assert_eq!(manager.balance_error.as_deref(), Some("request failed"));
    }

    #[test]
    fn changed_home_content_replaces_the_previous_result() {
        let mut manager = Manager::new(Vec::new().into());
        manager.home_content = Some(super::home::HomeContent {
            title: "Home".to_owned(),
            lines: vec!["ssh old-host".to_owned()],
        });
        let updated_content = super::home::HomeContent {
            title: "Home".to_owned(),
            lines: vec!["ssh new-host".to_owned()],
        };

        let _task = manager.update(Message::HomeLoaded(0, Ok(updated_content.clone())));

        assert_eq!(manager.home_content.as_ref(), Some(&updated_content));
    }

    #[test]
    fn collects_valuable_coin_prices_but_omits_usdt() {
        let account = AccountBalance {
            total_eq: "10000".to_owned(),
            details: vec![
                BalanceDetail {
                    ccy: "SOL".to_owned(),
                    eq: "2".to_owned(),
                    eq_usd: "300".to_owned(),
                },
                BalanceDetail {
                    ccy: "USDT".to_owned(),
                    eq: "100".to_owned(),
                    eq_usd: "100".to_owned(),
                },
                BalanceDetail {
                    ccy: "ETH".to_owned(),
                    eq: "2".to_owned(),
                    eq_usd: "600".to_owned(),
                },
                BalanceDetail {
                    ccy: "DUST".to_owned(),
                    eq: "1".to_owned(),
                    eq_usd: "0.5".to_owned(),
                },
                BalanceDetail {
                    ccy: "BTC".to_owned(),
                    eq: "1".to_owned(),
                    eq_usd: "1000".to_owned(),
                },
            ],
        };

        assert_eq!(
            collect_asset_prices(&[account]),
            vec![
                ("BTC".to_owned(), "1000.00 USD".to_owned()),
                ("ETH".to_owned(), "300.00 USD".to_owned()),
                ("SOL".to_owned(), "150.00 USD".to_owned()),
            ]
        );
    }

    #[test]
    fn balance_refresh_updates_prices_without_resetting_rotation() {
        let mut manager = Manager::new(Vec::new().into());
        manager.price_rotation_index = 2;
        let account = AccountBalance {
            total_eq: "10000".to_owned(),
            details: vec![
                BalanceDetail {
                    ccy: "BTC".to_owned(),
                    eq: "1".to_owned(),
                    eq_usd: "1000".to_owned(),
                },
                BalanceDetail {
                    ccy: "ETH".to_owned(),
                    eq: "2".to_owned(),
                    eq_usd: "800".to_owned(),
                },
                BalanceDetail {
                    ccy: "SOL".to_owned(),
                    eq: "3".to_owned(),
                    eq_usd: "900".to_owned(),
                },
                BalanceDetail {
                    ccy: "XRP".to_owned(),
                    eq: "4".to_owned(),
                    eq_usd: "400".to_owned(),
                },
            ],
        };

        let _task = manager.update(Message::BalanceLoaded(0, Ok(vec![account])));

        assert_eq!(manager.price_rotation_index, 2);
        assert_eq!(
            manager.visible_prices(),
            vec![
                &("SOL".to_owned(), "300.00 USD".to_owned()),
                &("XRP".to_owned(), "100.00 USD".to_owned()),
                &("BTC".to_owned(), "1000.00 USD".to_owned()),
            ]
        );

        let _task = manager.update(Message::RotatePrices);
        assert_eq!(manager.price_rotation_index, 3);
    }

    #[test]
    fn loads_drawers_from_config_file() {
        let drawers = load().expect("conf/layout.toml should load");
        assert!(!drawers.is_empty());
    }

    #[test]
    fn resolves_config_inside_macos_app_bundle() {
        let executable = Path::new("/Applications/Tie.app/Contents/MacOS/tie");
        assert_eq!(
            bundled_config_path(executable),
            Some(PathBuf::from(
                "/Applications/Tie.app/Contents/Resources/conf/layout.toml"
            ))
        );
    }
}

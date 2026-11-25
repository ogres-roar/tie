mod drawer;

use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use iced::widget::{button, center, column, container, mouse_area, row, text};
use iced::{Element, Length, Size, Subscription, Task, time, window};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub fn main() -> Result<(), Box<dyn Error>> {
    let drawers: Arc<[drawer::Drawer]> = load_drawers()?.into();

    iced::application(
        move || Closet::new(Arc::clone(&drawers)),
        Closet::update,
        Closet::view,
    )
    .title("Tie")
    .window_size(Size::new(300.0, 500.0))
    .decorations(false)
    .exit_on_close_request(false)
    .subscription(Closet::subscription)
    .run()?;

    Ok(())
}

struct Closet {
    window_id: Option<window::Id>,
    tray_icon: Option<TrayIcon>,
    window_hidden: bool,
    window_pinned: bool,
    drawers: Arc<[drawer::Drawer]>,
    balances: Option<Vec<drawer::AccountBalance>>,
    balance_error: Option<String>,
    balance_loading: bool,
}

#[derive(Debug, Clone)]
enum Message {
    WindowOpened(window::Id),
    CloseRequested(window::Id),
    PollTray,
    RefreshAccountData,
    DragWindow,
    HideWindow,
    TogglePin,
    BalanceLoaded(Result<Vec<drawer::AccountBalance>, String>),
}

impl Closet {
    fn new(drawers: Arc<[drawer::Drawer]>) -> Self {
        Self {
            window_id: None,
            tray_icon: None,
            window_hidden: false,
            window_pinned: false,
            drawers,
            balances: None,
            balance_error: None,
            balance_loading: false,
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowOpened(id) => {
                self.window_id = Some(id);

                #[cfg(target_os = "macos")]
                if let Err(error) = set_dock_icon() {
                    eprintln!("Failed to set macOS Dock icon: {error}");
                }

                if self.tray_icon.is_none() {
                    match create_tray_icon() {
                        Ok(icon) => self.tray_icon = Some(icon),
                        Err(error) => eprintln!("Failed to create system tray icon: {error}"),
                    }
                }

                self.refresh_account_data()
            }
            Message::CloseRequested(id) if Some(id) == self.window_id => self.hide_window(id),
            Message::CloseRequested(_) => Task::none(),
            Message::DragWindow => self.window_id.map_or_else(Task::none, window::drag),
            Message::HideWindow => self
                .window_id
                .map_or_else(Task::none, |id| self.hide_window(id)),
            Message::TogglePin => self.toggle_pin(),
            Message::PollTray => {
                let clicked = loop {
                    match TrayIconEvent::receiver().try_recv() {
                        Ok(TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }) => break true,
                        Ok(_) => continue,
                        Err(_) => break false,
                    }
                };

                if clicked && self.window_hidden {
                    self.show_window()
                } else {
                    Task::none()
                }
            }
            Message::RefreshAccountData => self.refresh_account_data(),
            Message::BalanceLoaded(result) => {
                self.balance_loading = false;
                match result {
                    Ok(balances) => {
                        self.balances = Some(balances);
                        self.balance_error = None;
                    }
                    Err(error) => {
                        self.balances = None;
                        self.balance_error = Some(error);
                    }
                }
                Task::none()
            }
        }
    }

    fn refresh_account_data(&mut self) -> Task<Message> {
        let Some(okx) = self.drawers.iter().find_map(|drawer| match drawer {
            drawer::Drawer::Okx(okx) => Some(okx.clone()),
            drawer::Drawer::Text(_) => None,
        }) else {
            return Task::none();
        };

        self.balance_loading = true;
        self.balance_error = None;
        Task::perform(okx.get_balance(), Message::BalanceLoaded)
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            window::open_events().map(Message::WindowOpened),
            window::close_requests().map(Message::CloseRequested),
            time::every(Duration::from_millis(100)).map(|_| Message::PollTray),
        ])
    }

    fn show_window(&mut self) -> Task<Message> {
        let Some(id) = self.window_id else {
            return Task::none();
        };

        self.window_hidden = false;
        Task::batch([
            window::set_mode(id, window::Mode::Windowed),
            window::gain_focus(id),
        ])
    }

    fn hide_window(&mut self, id: window::Id) -> Task<Message> {
        self.window_hidden = true;
        window::set_mode(id, window::Mode::Hidden)
    }

    fn toggle_pin(&mut self) -> Task<Message> {
        let Some(id) = self.window_id else {
            return Task::none();
        };

        self.window_pinned = !self.window_pinned;
        let level = if self.window_pinned {
            window::Level::AlwaysOnTop
        } else {
            window::Level::Normal
        };

        window::set_level(id, level)
    }

    fn view(&self) -> Element<'_, Message> {
        let title_bar = row![
            button("×").on_press(Message::HideWindow),
            mouse_area(
                container(text("Tie"))
                    .width(Length::Fill)
                    .height(Length::Fill)
            )
            .on_press(Message::DragWindow),
            button(if self.window_pinned {
                "取消置顶"
            } else {
                "置顶"
            })
            .on_press(Message::TogglePin),
        ]
        .height(40)
        .padding(4)
        .spacing(4);

        let mut content = column![
            text("OKX 账户余额"),
            button(if self.balance_loading {
                "查询中..."
            } else {
                "刷新账户数据"
            })
            .on_press(Message::RefreshAccountData)
        ];

        if self.balance_loading {
            content = content.push(text("正在查询账户余额..."));
        } else if let Some(error) = &self.balance_error {
            content = content.push(text(error));
        } else if let Some(accounts) = &self.balances {
            for account in accounts {
                match account.total_eq.parse::<f64>() {
                    Ok(total_eq) if total_eq.is_finite() => {
                        content = content.push(text(format!("账户总余额 (USD): {total_eq:.2}")));
                    }
                    Ok(_) => {
                        content = content.push(text("账户总余额格式无效"));
                    }
                    Err(error) => {
                        content = content.push(text(format!("无法解析账户总余额: {error}")));
                    }
                }

                for balance in &account.details {
                    match balance.spot_price_usd() {
                        Ok(Some(price)) => {
                            content = content
                                .push(text(format!("{} 现货价格 (USD): {price:.2}", balance.ccy)));
                        }
                        Ok(None) => {}
                        Err(error) => content = content.push(text(error)),
                    }
                }
            }
        }

        column![title_bar, center(content).padding(20)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

fn load_drawers() -> Result<Vec<drawer::Drawer>, Box<dyn Error>> {
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
    let layout: drawer::Layout = toml::from_str(&contents).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse {}: {error}", path.display()),
        )
    })?;

    Ok(layout.item)
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
    use super::{bundled_config_path, load_drawers};
    use std::path::{Path, PathBuf};

    #[test]
    fn loads_drawers_from_config_file() {
        let drawers = load_drawers().expect("conf/layout.toml should load");
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

fn create_tray_icon() -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let mut rgba = vec![0; 16 * 16 * 4];

    for y in 2..14 {
        for x in 3..13 {
            if y < 5 || (7..9).contains(&x) {
                let pixel = (y * 16 + x) * 4;
                rgba[pixel..pixel + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
    }

    let icon = Icon::from_rgba(rgba, 16, 16)?;
    let builder = TrayIconBuilder::new().with_tooltip("tie").with_icon(icon);

    #[cfg(target_os = "macos")]
    let builder = builder.with_icon_as_template(true);

    Ok(builder.with_menu_on_left_click(false).build()?)
}

#[cfg(target_os = "macos")]
fn set_dock_icon() -> Result<(), &'static str> {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let main_thread = MainThreadMarker::new().ok_or("must be called on the main thread")?;
    let data = NSData::with_bytes(include_bytes!("../assets/tie-icon.png"));
    let image = NSImage::initWithData(NSImage::alloc(), &data)
        .ok_or("the embedded PNG icon could not be decoded")?;

    unsafe {
        NSApplication::sharedApplication(main_thread).setApplicationIconImage(Some(&image));
    }

    Ok(())
}

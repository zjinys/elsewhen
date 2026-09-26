use crate::{event::NewEvent, storage::Store};
use anyhow::Result;
use iced::widget::{button, container, operation, row, svg, text_input};
use iced::{keyboard, Element, Length, Subscription, Task, Theme};

const INPUT_ID: &str = "capture-input";

/// Square settings button, sized to match the input height
/// (text size 21 + 16 vertical padding on each side ≈ 60px).
const SETTINGS_SIZE: f32 = 60.0;
const GEAR_SIZE: f32 = 32.0;

/// Gear icon (Material `settings`, filled) rendered as vector graphics so its
/// size does not depend on which fonts are installed. Fill matches the
/// TokyoNight primary-button text (`#1a1b26` on `#2ac3de`).
const GEAR_ICON: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="#1a1b26"><path d="M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58c.18-.14.23-.41.12-.61l-1.92-3.32c-.12-.22-.37-.29-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54c-.04-.24-.24-.41-.5-.41h-3.8c-.27 0-.46.17-.5.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.7 8.65c-.11.2-.06.47.12.61l2.03 1.58c-.05.3-.07.62-.07.94s.02.64.07.94l-2.03 1.58c-.18.14-.23.41-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.5.41h3.8c.27 0 .46-.17.5-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.11-.2.06-.47-.12-.61l-2.01-1.58zM12 15.6c-1.98 0-3.6-1.62-3.6-3.6s1.62-3.6 3.6-3.6 3.6 1.62 3.6 3.6-1.62 3.6-3.6 3.6z"/></svg>"##;

pub fn run(store: Store) -> Result<()> {
    iced::application(move || Capture::new(store.clone()), update, view)
        .title("Elsewhen")
        .theme(capture_theme)
        .subscription(subscription)
        .window_size((720.0, 108.0))
        .centered()
        .resizable(false)
        .decorations(false)
        .run()
        .map_err(Into::into)
}

struct Capture {
    text: String,
    store: Store,
    error: Option<String>,
}

#[derive(Debug, Clone)]
enum Message {
    Changed(String),
    Submit,
    Cancel,
    Settings,
    Event(iced::Event),
}

impl Capture {
    fn new(store: Store) -> (Self, Task<Message>) {
        (
            Self {
                text: String::new(),
                store,
                error: None,
            },
            operation::focus(INPUT_ID),
        )
    }
}

fn update(state: &mut Capture, message: Message) -> Task<Message> {
    match message {
        Message::Changed(value) => state.text = value,
        Message::Submit => {
            let value = state.text.trim();
            if value.is_empty() {
                return Task::none();
            }
            match state.store.insert_event(NewEvent::now(value)) {
                // 写库失败必须在窗口内显示原因，否则用户以为已记录、事件静默丢失。
                Err(e) => {
                    state.error = Some(format!("保存失败：{e}"));
                }
                Ok(_) => {
                    spawn_background_analysis();
                    std::process::exit(0);
                }
            }
        }
        Message::Cancel => std::process::exit(0),
        Message::Settings => {
            if let Ok(executable) = std::env::current_exe() {
                let _ = std::process::Command::new(executable)
                    .arg("settings")
                    .spawn();
            }
            std::process::exit(0);
        }
        Message::Event(iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Enter),
            ..
        })) => return Task::done(Message::Submit),
        Message::Event(iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            ..
        })) => return Task::done(Message::Cancel),
        _ => {}
    }
    Task::none()
}

fn spawn_background_analysis() {
    if let Ok(executable) = std::env::current_exe() {
        let _ = std::process::Command::new(executable)
            .arg("analyze-once")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

fn subscription(_: &Capture) -> Subscription<Message> {
    iced::event::listen().map(Message::Event)
}
fn capture_theme(_: &Capture) -> Theme {
    Theme::TokyoNight
}

fn view(state: &Capture) -> Element<'_, Message> {
    let field = text_input("记录发生了什么…", &state.text)
        .id(INPUT_ID)
        .on_input(Message::Changed)
        .on_submit(Message::Submit)
        .padding(16)
        .size(21)
        .width(Length::Fill);
    let settings = button(
        svg(svg::Handle::from_memory(GEAR_ICON))
            .width(Length::Fixed(GEAR_SIZE))
            .height(Length::Fixed(GEAR_SIZE)),
    )
    .on_press(Message::Settings)
    .padding(8)
    .width(Length::Fixed(SETTINGS_SIZE))
    .height(Length::Fixed(SETTINGS_SIZE));
    let bar = row![field, settings]
        .spacing(10)
        .align_y(iced::Alignment::Center);
    let mut column = iced::widget::column![bar].spacing(6);
    if let Some(error) = &state.error {
        column = column.push(
            container(iced::widget::text(error).size(12))
                .padding([0, 4])
                .style(|theme: &Theme| {
                    container::Style {
                        text_color: Some(theme.extended_palette().danger.base),
                        ..Default::default()
                    }
                }),
        );
    }
    container(column)
        .padding(10)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_y(Length::Fill)
        .into()
}

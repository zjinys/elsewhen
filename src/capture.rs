use crate::{event::NewEvent, storage::Store};
use anyhow::Result;
use iced::{Element, Length, Subscription, Task, Theme, keyboard};
use iced::widget::{button, column, container, row, text, text_input};

pub fn run(store: Store) -> Result<()> {
    iced::application(move || Capture::new(store.clone()), update, view)
        .title("Elsewhen")
        .theme(capture_theme)
        .subscription(subscription)
        .window_size((720.0, 390.0))
        .centered()
        .resizable(false)
        .decorations(false)
        .run()
        .map_err(Into::into)
}

struct Capture { text: String, store: Store }

#[derive(Debug, Clone)] enum Message { Changed(String), Submit, Cancel, Event(iced::Event) }

impl Capture { fn new(store: Store) -> Self { Self { text: String::new(), store } } }

fn update(state: &mut Capture, message: Message) -> Task<Message> {
    match message {
        Message::Changed(value) => state.text = value,
        Message::Submit => {
            let value = state.text.trim();
            if !value.is_empty() && state.store.insert_event(NewEvent::now(value)).is_ok() { std::process::exit(0); }
        }
        Message::Cancel => std::process::exit(0),
        Message::Event(iced::Event::Keyboard(keyboard::Event::KeyPressed { key: keyboard::Key::Named(keyboard::key::Named::Enter), .. })) => return Task::done(Message::Submit),
        Message::Event(iced::Event::Keyboard(keyboard::Event::KeyPressed { key: keyboard::Key::Named(keyboard::key::Named::Escape), .. })) => return Task::done(Message::Cancel),
        _ => {}
    }
    Task::none()
}

fn subscription(_: &Capture) -> Subscription<Message> { iced::event::listen().map(Message::Event) }
fn capture_theme(_: &Capture) -> Theme { Theme::TokyoNight }

fn view(state: &Capture) -> Element<'_, Message> {
    let field = text_input("例如：今天和客户确认了项目，报价还没谈", &state.text)
        .id("capture-input").on_input(Message::Changed).on_submit(Message::Submit)
        .padding(18).size(20).width(Length::Fill);
    let actions = row![button("取消").on_press(Message::Cancel).padding([11, 22]), button("保存记录").on_press(Message::Submit).padding([11, 24])].spacing(12);
    let card = column![
        text("ELSEWHEN").size(12),
        text("记下此刻，稍后再整理").size(30),
        text("只需写发生了什么，不必分类或打标签").size(15),
        field,
        row![text("Enter 保存  ·  Esc 取消").size(12), iced::widget::Space::new().width(Length::Fill), actions].align_y(iced::Alignment::Center)
    ].spacing(18).padding(38).width(Length::Fill);
    container(card).width(Length::Fill).height(Length::Fill).center_y(Length::Fill).into()
}

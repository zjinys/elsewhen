use anyhow::Result;
use iced::widget::{button, column, container, row, text};
use iced::{Element, Length, Task, Theme};
use std::path::PathBuf;

pub fn run(database_path: PathBuf) -> Result<()> {
    iced::application(
        move || SettingsState::new(database_path.clone()),
        update,
        view,
    )
    .title("Elsewhen 设置")
    .theme(settings_theme)
    .window_size((560.0, 330.0))
    .centered()
    .resizable(false)
    .run()
    .map_err(Into::into)
}

struct SettingsState {
    database_path: PathBuf,
    executable: String,
    wayland: bool,
}

#[derive(Debug, Clone)]
enum Message {
    Close,
}

impl SettingsState {
    fn new(database_path: PathBuf) -> Self {
        let executable = std::env::current_exe()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "elsewhen".into());
        let wayland = std::env::var("XDG_SESSION_TYPE").is_ok_and(|value| value == "wayland");
        Self {
            database_path,
            executable,
            wayland,
        }
    }

    fn capture_command(&self) -> String {
        format!("{} capture", self.executable)
    }
}

fn update(_: &mut SettingsState, message: Message) -> Task<Message> {
    match message {
        Message::Close => std::process::exit(0),
    }
}

fn settings_theme(_: &SettingsState) -> Theme {
    Theme::TokyoNight
}

fn view(state: &SettingsState) -> Element<'_, Message> {
    let shortcut_note = if state.wayland {
        "当前为 Wayland。请在 KDE 系统设置 → 快捷键 → 自定义快捷键中，将 Meta+Space 绑定到下面的命令。"
    } else {
        "当前会话支持运行 elsewhen daemon 监听双击 Left Ctrl。"
    };
    let command = state.capture_command();
    let content = column![
        text("Elsewhen 设置").size(26),
        text("全局快捷键").size(16),
        text(shortcut_note).size(14),
        container(text(command).size(14))
            .padding(12)
            .width(Length::Fill),
        text("数据文件").size(16),
        container(text(state.database_path.display().to_string()).size(13))
            .padding(12)
            .width(Length::Fill),
        row![
            iced::widget::Space::new().width(Length::Fill),
            button("完成").on_press(Message::Close).padding([10, 22])
        ]
    ]
    .spacing(14)
    .padding(28);
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

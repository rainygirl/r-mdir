mod app;
mod config;
#[cfg(target_os = "haiku")]
mod haiku;
mod remote;
mod ui;

use std::{io, panic};

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, SetTitle, disable_raw_mode, enable_raw_mode,
    },
};
use ratatui::Terminal;

#[tokio::main]
async fn main() -> Result<()> {
    let old_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        old_hook(info);
    }));

    enable_raw_mode()?;
    // 터미널 창/탭 제목을 지정한다. 안 하면 셸이나 실행 스크립트가 정한 제목(예: 실행
    // 파일 이름 "m")이 그대로 남아, 화면 캡처나 작업 표시줄에 "mdir" 대신 다른 이름이
    // 뜨는 경우가 있었다. OSC 이스케이프라 macOS/Linux/Haiku Terminal 모두에서 먹힌다.
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        SetTitle("R Mdir")
    )?;

    let result = run_tui().await;
    let restored = restore_terminal();
    result.and(restored)
}

async fn run_tui() -> Result<()> {
    let backend = ratatui::backend::CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    app::App::new().run(&mut terminal).await
}

fn restore_terminal() -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}

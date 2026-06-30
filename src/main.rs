use tracing::info;
use tracing_subscriber;
use calloop::EventLoop;
use smithay::reexports::wayland_server::Display;

fn main() {
    // sistema de logs
    tracing_subscriber::fmt::init();
    info!("iniciando vinland...");

    let mut event_loop: EventLoop<()> = EventLoop::try_new()
        .expect("failed to initialize event loop");

    let mut display: Display<()> = Display::new().unwrap();

    let _display_handle = display.handle();
    info!("display wayland creado");

    let loop_handle = event_loop.handle();

loop_handle
    .insert_source(
        smithay::reexports::calloop::generic::Generic::new(
            display,
            calloop::Interest::READ,
            calloop::Mode::Level,
        ),
        |_, _, state| {
            Ok(calloop::PostAction::Continue)
        },
    )
    .unwrap();

    info!("Display conectado al event loop");
    event_loop.run(None, &mut (), |_| {}).unwrap();
}
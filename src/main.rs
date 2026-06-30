use tracing::info;
use tracing_subscriber;
use calloop::EventLoop;
use smithay::reexports::wayland_server::Display;

fn main() {
    // sistema de logs
    tracing_subscriber::fmt::init();
    info!("iniciando vinland...");

    let mut display: Display<()> = Display::new().unwrap();
    info!("display wayland creado :3");

    let mut event_loop: EventLoop<()> = EventLoop::try_new()
        .expect("failed to initialize event loop");

    info!("placeholder eventloop :3");

    event_loop.run(None, &mut (), |_| {}).unwrap();
}
use tracing::info;
use tracing_subscriber;

fn main() {
    // sistema de logs
    tracing_subscriber::fmt::init();
    
    info!("iniciando vinland...");
}
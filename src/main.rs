mod app;
mod state;
mod texture;

use app::run;

fn main() {
    pollster::block_on(run());
}

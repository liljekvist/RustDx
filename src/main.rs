mod app;

use app::run;

fn main() {
    pollster::block_on(run());
}

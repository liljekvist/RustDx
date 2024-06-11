mod app;
mod state;
mod texture;
mod camera;
mod cameraController;

use app::run;

fn main() {
    pollster::block_on(run());
}

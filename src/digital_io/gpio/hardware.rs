pub use gpio_cdev::*;

pub fn find_line(name: &str) -> Option<Line> {
    chips()
        .unwrap()
        .flat_map(|c| c.unwrap().lines())
        .find(|l| l.info().unwrap().name() == Some(name))
}

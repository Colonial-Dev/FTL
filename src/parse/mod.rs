pub mod delimit;
pub mod link;
pub mod shortcode;

use std::ops::Range;

pub trait Ranged {
    fn range(&self) -> Range<usize>;

    fn start(&self) -> usize {
        self.range().start
    }

    fn end(&self) -> usize {
        self.range().end
    }
}

//! The timings a host may ask of `UiRenderer::prepare_pages`
//! (`UiRenderer::set_prepare_timings`).
//!
//! Needed because a host that hunts a slow frame wants to know which part
//! of `prepare_pages` took the time, measured with ITS clock and counters
//! (a frame profiler, an allocation counter), and this crate must not pick
//! those for it. A host that asks for no timings pays nothing: no clock is
//! read.

/// One part of `UiRenderer::prepare_pages`, in the order they run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrepareStage {
    /// The walk over the draw lists: every quad of the frame is built and
    /// the text is shaped. Runs twice in a frame whose text grew a glyph
    /// atlas; both runs are inside one begin / end pair.
    Pass,
    /// The glyph atlases and the gradient tables go to the GPU, when they
    /// changed.
    Atlas,
    /// The frame's quads and the screen size go to the GPU, when they
    /// changed.
    Upload,
}

/// Told as `UiRenderer::prepare_pages` enters and leaves each stage, so
/// the host can time them.
///
/// Every method does nothing unless the host writes it, so a host that
/// only wants the text time implements `text_shaped` alone.
///
/// `Send + Sync`, so that a renderer holding one can still be built on
/// one thread and handed to another, as one without can. No lock is
/// needed for that: every call hands it `&mut self`.
pub trait PrepareTimings: Send + Sync {
    /// `stage` starts now.
    fn stage_begins(&mut self, _stage: PrepareStage) {}

    /// `stage` is over. Always follows the `stage_begins` of the same
    /// stage, with no other stage between them.
    fn stage_ends(&mut self, _stage: PrepareStage) {}

    /// The seconds one walk spent shaping text, a part of
    /// `PrepareStage::Pass`. Called once per walk, so twice in a frame
    /// that walked twice.
    fn text_shaped(&mut self, _seconds: f32) {}
}

use std::any::Any;

use super::context::{PostSimCtx, PreSimCtx};
use crate::component::Component;
use crate::deps::arcstr::ArcStr;
use crate::error::Result;

pub trait Testbench: Component + Any {
    type Output;

    /// Declares the name of the global ground net.
    fn ground_net(&self) -> ArcStr {
        arcstr::literal!("vss")
    }

    /// Called before the generated netlist is simulated.
    /// Can be used to set simulator analyses, add includes, write PWL files, etc.
    #[allow(unused_variables)]
    fn setup(&mut self, ctx: &mut PreSimCtx) -> Result<()> {
        Ok(())
    }

    /// Called after simulation completes. Included for consistency,
    /// but post-simulation computation can be done in `measure` instead.
    #[allow(unused_variables)]
    fn post_sim(&mut self, ctx: &mut PostSimCtx) -> Result<()> {
        Ok(())
    }

    /// Processes simulation data and extracts the desired measurements.
    #[allow(unused_variables)]
    fn measure(&mut self, ctx: &PostSimCtx) -> Result<Self::Output>;

    /// Cleans up any files generated by the testbench.
    fn cleanup(&mut self) {}
}

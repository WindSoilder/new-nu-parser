mod declaration;
mod def;
mod signature;
mod prelude;

use prelude::*;

pub trait Command: CommandClone {
    fn name(&self) -> &str;
    fn description(&self) -> &str {
        ""
    }
    fn signature(&self) -> Signature;
    fn extra_description(&self) -> &str {
        ""
    }
    fn command_type(&self) -> CommandType {
        CommandType::Builtin
    }
    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError>;
    fn examples(&self) -> Vec<Example<'_>> {
        vec![]
    }
}

// Cloning implementation taken from Nushell
pub trait CommandClone {
    fn clone_box(&self) -> Box<dyn Command>;
}

impl<T> CommandClone for T
where
    T: 'static + Command + Clone,
{
    fn clone_box(&self) -> Box<dyn Command> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn Command> {
    fn clone(&self) -> Box<dyn Command> {
        self.clone_box()
    }
}

pub use declaration::Declaration;

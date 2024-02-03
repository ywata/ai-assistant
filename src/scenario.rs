use serde::{Deserialize};


#[derive(Clone, Debug, Default, Deserialize)]
pub struct Input{
    pub tag: String,
    pub text: String,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Prompt {
    pub instruction: String,
    pub inputs: Vec<Input>,
}



impl Prompt {
    pub fn new(instruction: String, inputs: Vec<Input>) -> Self {
        Prompt { instruction, inputs }
    }
    pub fn get_instruction(&self) -> String {
        self.instruction.clone()
    }
    pub fn get_inputs(&self) -> Vec<Input> {
        self.inputs.clone()
    }
}
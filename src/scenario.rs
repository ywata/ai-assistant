use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Input {
    pub tag: String,
    pub prefix: Option<String>,
    pub text: String,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Prompt {
    pub instruction: String,
    pub inputs: Vec<Input>,
}

impl Prompt {
    pub fn new(instruction: String, inputs: Vec<Input>) -> Self {
        Prompt {
            instruction,
            inputs,
        }
    }
    pub fn get_instruction(&self) -> String {
        self.instruction.clone()
    }
    pub fn get_inputs(&self) -> Vec<Input> {
        self.inputs.clone()
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub enum StateTrans {
    #[default]
    Stop,
    Next {
        name: String,
        tag: String,
    },
}

pub enum Directive {
    KeepAsIs,
}
pub trait Renderer<S, T> {
    fn render(state: &S) -> T;
}

#[derive(Debug, Default, Deserialize)]
pub struct Item<S, T, I, O>
where
    S: Debug,
    T: Debug,
    I: Renderer<S, T> + Clone + Debug + Default,
    O: Renderer<S, T> + Clone + Debug + Default,
{
    #[serde(skip)]
    _s: PhantomData<S>,
    #[serde(skip)]
    _t: PhantomData<T>,
    next: StateTrans,
    request: Box<I>,
    response: Box<O>,
}

impl<S, T, I, O> Clone for Item<S, T, I, O>
where
    S: Debug,
    T: Debug,
    I: Renderer<S, T> + Clone + Debug + Default,
    O: Renderer<S, T> + Clone + Debug + Default,
{
    fn clone(&self) -> Self {
        Item {
            _s: PhantomData,
            _t: PhantomData,
            next: self.next.clone(),
            request: self.request.clone(),
            response: self.response.clone(),
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Workflow<S, T, I, O>
where
    S: Debug,
    T: Debug,
    I: Renderer<S, T> + Clone + Debug + Default,
    O: Renderer<S, T> + Clone + Debug + Default,
{
    pub workflow: HashMap<String, HashMap<String, Item<S, T, I, O>>>,
}

impl<S, T, I, O> Workflow<S, T, I, O>
where
    S: Debug,
    T: Clone + Debug,
    I: Renderer<S, T> + Clone + Debug + Default,
    O: Renderer<S, T> + Clone + Debug + Default,
{
    pub fn new(workflow: HashMap<String, HashMap<String, Item<S, T, I, O>>>) -> Self {
        Workflow { workflow }
    }
}

fn list_input_identifier(prompts: &HashMap<String, Prompt>) -> Vec<(String, String)> {
    let mut vec = Vec::new();
    for (name, prompt) in prompts.iter() {
        for input in prompt.inputs.iter() {
            vec.push((name.clone(), input.tag.clone()));
        }
    }
    vec
}

use log::debug;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::marker::PhantomData;

type Tag = String;
type Name = String;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Input {
    pub prefix: Option<String>,
    pub text: String,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Prompt {
    pub instruction: String,
    pub inputs: HashMap<Tag, Input>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub enum StateTrans {
    #[default]
    Stop,
    Next {
        name: Name,
        tag: Tag,
    },
}

pub trait Renderer<S, T> {
    fn render(&self, state: &S) -> T;
}

#[derive(Debug, Default, Deserialize)]
pub struct Item<S, T, I, O>
where
    S: Debug,
    T: Debug,
    I: Renderer<S, T> + Clone + Debug,
    O: Renderer<S, T> + Clone + Debug,
{
    #[serde(skip)]
    pub _s: PhantomData<S>,
    #[serde(skip)]
    pub _t: PhantomData<T>,
    pub next: StateTrans,
    pub request: Box<I>,
    pub response: Box<O>,
}

impl<S, T, I, O> Clone for Item<S, T, I, O>
where
    S: Debug,
    T: Debug,
    I: Renderer<S, T> + Clone + Debug,
    O: Renderer<S, T> + Clone + Debug,
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
    I: Renderer<S, T> + Clone + Debug,
    O: Renderer<S, T> + Clone + Debug,
{
    pub workflow: HashMap<String, HashMap<String, Item<S, T, I, O>>>,
}

pub fn parse_scenario<S, T, I, O>(
    prompts: HashMap<String, Box<Prompt>>,
    wf: Workflow<S, T, I, O>,
) -> Option<(HashMap<String, Box<Prompt>>, Workflow<S, T, I, O>)>
where
    S: Debug,
    T: Debug,
    I: Renderer<S, T> + Clone + Debug,
    O: Renderer<S, T> + Clone + Debug,
{
    if prompts.is_empty() {
        return None;
    }
    let prompt_pairs: HashSet<(String, String)> = prompts
        .iter()
        .map(|(n, p)| p.inputs.iter().map(|(t, _i)| (n.clone(), t.clone())))
        .flatten()
        .collect();

    let wf_pairs: HashSet<(String, String)> = wf
        .workflow
        .iter()
        .map(|(n, hm)| hm.iter().map(|(t, _itm)| (n.clone(), t.clone())))
        .flatten()
        .collect();
    if prompt_pairs.eq(&wf_pairs) {
        Some((prompts, wf))
    } else {
        debug!("{:?}", prompt_pairs);
        debug!("{:?}", wf_pairs);
        None
    }
}

impl<S, T, I, O> Workflow<S, T, I, O>
where
    S: Debug,
    T: Debug,
    I: Renderer<S, T> + Clone + Debug,
    O: Renderer<S, T> + Clone + Debug,
{
    pub fn get_item(&self, name: &str, tag: &str) -> Option<&Item<S, T, I, O>> {
        self.workflow.get(name).map(|hm| hm.get(tag)).flatten()
    }
}

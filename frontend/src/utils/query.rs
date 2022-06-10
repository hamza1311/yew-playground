use serde::{Deserialize, Serialize};
use yew::prelude::*;
use yew_router::hooks::use_location;

#[derive(Debug,Serialize, Deserialize, PartialEq)]
pub struct Query {
    pub shared: Option<String>,
}

#[hook]
pub fn use_query() -> Option<Query> {
    let location = use_location()?;
    location.query::<Query>().ok()
}

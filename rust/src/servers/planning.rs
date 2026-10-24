use super::manifest::Manifest;
use crate::{
    metadata::Error,
    plan::{Plan, Target},
    version,
};
pub trait Planner {
    fn resolve(&self, manifest: &Manifest) -> Result<(), Error>;
    fn plan(&self, manifest: &Manifest, target: Target) -> Result<Plan, Error>;
}
impl<T: version::Transport> Planner for version::Service<T> {
    fn resolve(&self, m: &Manifest) -> Result<(), Error> {
        self.resolve_server_selection(m)
    }
    fn plan(&self, m: &Manifest, t: Target) -> Result<Plan, Error> {
        self.plan_server(m, t)
    }
}
pub fn fabric_args(files: &[crate::plan::File]) -> Result<Vec<u8>, &'static str> {
    if files.first().is_none_or(|f| f.path != "server.jar") {
        return Err("InvalidFabricServerPlan");
    }
    let mut args = String::from("-cp ");
    for (i, file) in files[1..].iter().enumerate() {
        if !file.path.starts_with("libraries/") {
            return Err("InvalidFabricServerPlan");
        }
        if i != 0 {
            args.push(':');
        }
        args.push_str(&file.path);
    }
    args.push_str(":server.jar\nnet.fabricmc.loader.impl.launch.knot.KnotServer\n");
    Ok(args.into_bytes())
}

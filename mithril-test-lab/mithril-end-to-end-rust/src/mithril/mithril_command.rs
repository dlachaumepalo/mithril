use slog_scope::info;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};

#[derive(Debug)]
pub struct MithrilCommand {
    name: String,
    process_path: PathBuf,
    log_path: PathBuf,
    work_dir: PathBuf,
    env_vars: HashMap<String, String>,
    default_args: Vec<String>,
}

impl MithrilCommand {
    pub fn new(
        name: &str,
        work_dir: &Path,
        bin_dir: &Path,
        env_vars: HashMap<&str, &str>,
        default_args: &[&str],
    ) -> Result<MithrilCommand, String> {
        let process_path = bin_dir.canonicalize().unwrap().join(&name);
        let log_path = work_dir.join(format!("{}.log", name));

        // ugly but it's far easier for callers to manipulate string literals
        let mut env_vars: HashMap<String, String> = env_vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let default_args = default_args.iter().map(|s| s.to_string()).collect();

        env_vars.insert("RUST_BACKTRACE".to_string(), "1".to_string());

        if !process_path.exists() {
            return Err(format!(
                "cannot find {} executable in expected location \"{}\"",
                name,
                bin_dir.display()
            ));
        }

        Ok(MithrilCommand {
            name: name.to_string(),
            process_path,
            log_path,
            work_dir: work_dir.to_path_buf(),
            env_vars,
            default_args,
        })
    }

    pub fn start(&mut self, args: &[String]) -> Child {
        let args = [&self.default_args, args].concat();

        let log_file_stdout = std::fs::File::options()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .unwrap();
        let log_file_stderr = log_file_stdout.try_clone().unwrap();

        let mut command = Command::new(&self.process_path);
        command
            .current_dir(&self.work_dir)
            .stdout(log_file_stdout)
            .stderr(log_file_stderr)
            .envs(&self.env_vars)
            .args(&args)
            .kill_on_drop(true);

        info!("Starting {}", self.name; "work_dir" => &self.work_dir.display(), "env" => #?&self.env_vars, "args" => #?&args);

        command
            .spawn()
            .unwrap_or_else(|_| panic!("{} failed to start", self.name))
    }

    pub(crate) async fn dump_logs_to_stdout(&self) -> Result<(), String> {
        if !self.log_path.exists() {
            return Err(format!(
                "No log for {}, did you run the command at least once ? expected path: {}",
                self.name,
                self.log_path.display()
            ));
        }

        let buffer = tokio::fs::read(&self.log_path).await.map_err(|e| {
            format!(
                "failed to read logfile `{}`: {}",
                self.log_path.display(),
                e
            )
        })?;

        tokio::io::stdout()
            .write_all(&buffer)
            .await
            .map_err(|e| format!("failed to dump {} logs: {}", &self.name, e))?;

        Ok(())
    }
}

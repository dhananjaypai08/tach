use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf, MAIN_SEPARATOR, MAIN_SEPARATOR_STR};

use crate::exclusion::is_path_excluded;

#[derive(Debug, Clone)]
pub struct FileSystemError {
    pub message: String,
}

impl fmt::Display for FileSystemError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.message)
    }
}

pub type Result<T> = std::result::Result<T, FileSystemError>;

pub fn canonical(root: &str, path: &str) -> Result<PathBuf> {
    let root = Path::new(root);
    let file_path = root.join(path);
    file_path.canonicalize().map_err(|_| FileSystemError {
        message: format!("Failed to canonicalize path: {}", path),
    })
}

pub fn file_to_module_path(file_path: &str) -> String {
    let file_path = file_path.trim_start_matches("./");

    if file_path == "." {
        return String::new();
    }

    let module_path = file_path.replace(MAIN_SEPARATOR, ".");

    let mut module_path = if module_path.ends_with(".py") {
        module_path.trim_end_matches(".py").to_string()
    } else {
        module_path
    };

    if module_path.ends_with(".__init__") {
        module_path.truncate(module_path.len() - 9);
    }

    if module_path == "__init__" {
        return String::new();
    }

    module_path
}

pub struct ResolvedModule {
    pub file_path: PathBuf,
    pub member_name: Option<String>,
}

pub fn module_to_file_path<P: AsRef<Path>>(root: P, mod_path: &str) -> Option<ResolvedModule> {
    let mut file_path = mod_path.replace(".", MAIN_SEPARATOR_STR);
    let fs_path = root.as_ref().join(file_path);
    file_path = fs_path.display().to_string();

    // mod_path may refer to a package
    if fs_path.join("__init__.py").exists() {
        return Some(ResolvedModule {
            file_path: fs_path,
            member_name: None,
        });
    }

    // mod_path may refer to a file
    let py_file_path = format!("{}.py", &file_path);
    if Path::new(&py_file_path).exists() {
        return Some(ResolvedModule {
            file_path: PathBuf::from(py_file_path),
            member_name: None,
        });
    }

    if let Some(last_sep_index) = file_path.rfind(MAIN_SEPARATOR) {
        // mod_path may refer to a member within a file
        let py_file_path = format!("{}.py", file_path[..last_sep_index].to_string());
        if Path::new(&py_file_path).exists() {
            let member_name = file_path[last_sep_index + 1..].to_string();
            return Some(ResolvedModule {
                file_path: PathBuf::from(py_file_path),
                member_name: Some(member_name),
            });
        }

        // mod_path may refer to a member within a package
        let init_py_file_path = format!(
            "{}{}__init__.py",
            file_path[..last_sep_index].to_string(),
            MAIN_SEPARATOR
        );
        if Path::new(&init_py_file_path).exists() {
            let member_name = file_path[last_sep_index + 1..].to_string();
            return Some(ResolvedModule {
                file_path: PathBuf::from(init_py_file_path),
                member_name: Some(member_name),
            });
        }
    }

    None
}

pub fn read_file_content<P: AsRef<Path>>(path: P) -> Result<String> {
    let mut file = fs::File::open(path.as_ref()).map_err(|_| FileSystemError {
        message: format!("Could not open path: {}", path.as_ref().display()),
    })?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|_| FileSystemError {
            message: format!("Could not read path: {}", path.as_ref().display()),
        })?;
    Ok(content)
}

pub fn is_project_import<P: AsRef<Path>>(root: P, mod_path: &str) -> Result<bool> {
    let resolved_module = module_to_file_path(root, mod_path);
    if let Some(module) = resolved_module {
        // This appears to be a project import, verify it is not excluded
        return match is_path_excluded(module.file_path.to_str().unwrap()) {
            Ok(true) => Ok(false),
            Ok(false) => Ok(true),
            Err(_) => Err(FileSystemError {
                message: "Failed to check if path is excluded".to_string(),
            }),
        };
    } else {
        // This is not a project import
        return Ok(false);
    }
}

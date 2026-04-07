use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedFileKind {
    Strm { remote_path: String, file_id: i64 },
    Subtitle { file_id: i64, remote_size: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    pub local_path: String,
    pub kind: PlannedFileKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalNode {
    pub path: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPlan {
    pub files: Vec<PlannedFile>,
    pub stale_files: Vec<String>,
    pub stale_dirs: Vec<String>,
}

pub fn build_sync_plan(
    mut files: Vec<PlannedFile>,
    expected_dirs: Vec<String>,
    local_nodes: Vec<LocalNode>,
) -> SyncPlan {
    files.sort_by(|left, right| left.local_path.cmp(&right.local_path));

    let expected_files = files
        .iter()
        .map(|file| file.local_path.as_str())
        .collect::<HashSet<_>>();
    let expected_dirs = expected_dirs.into_iter().collect::<HashSet<_>>();

    let mut stale_files = Vec::new();
    let mut stale_dirs = Vec::new();
    for node in local_nodes {
        if node.is_dir {
            if !expected_dirs.contains(&node.path) {
                stale_dirs.push(node.path);
            }
        } else if !expected_files.contains(node.path.as_str()) {
            stale_files.push(node.path);
        }
    }

    stale_files.sort();
    stale_dirs.sort();
    stale_dirs = collapse_nested_dirs(stale_dirs);

    SyncPlan {
        files,
        stale_files,
        stale_dirs,
    }
}

fn collapse_nested_dirs(mut dirs: Vec<String>) -> Vec<String> {
    dirs.sort();
    let mut retained = Vec::new();
    for dir in dirs {
        if retained
            .iter()
            .any(|ancestor: &String| dir == *ancestor || dir.starts_with(&(ancestor.clone() + "/")))
        {
            continue;
        }
        retained.push(dir);
    }
    retained
}

#[cfg(test)]
mod tests {
    use super::{LocalNode, PlannedFile, PlannedFileKind, build_sync_plan};

    #[test]
    fn marks_stale_files_and_top_level_dirs_only() {
        let plan = build_sync_plan(
            vec![PlannedFile {
                local_path: "/library/show/ep01.strm".to_string(),
                kind: PlannedFileKind::Strm {
                    remote_path: "/remote/show/ep01.mkv".to_string(),
                    file_id: 1,
                },
            }],
            vec!["/library".to_string(), "/library/show".to_string()],
            vec![
                LocalNode {
                    path: "/library/show".to_string(),
                    is_dir: true,
                },
                LocalNode {
                    path: "/library/show/ep01.strm".to_string(),
                    is_dir: false,
                },
                LocalNode {
                    path: "/library/show/old.srt".to_string(),
                    is_dir: false,
                },
                LocalNode {
                    path: "/library/obsolete".to_string(),
                    is_dir: true,
                },
                LocalNode {
                    path: "/library/obsolete/nested".to_string(),
                    is_dir: true,
                },
            ],
        );

        assert_eq!(plan.stale_files, vec!["/library/show/old.srt"]);
        assert_eq!(plan.stale_dirs, vec!["/library/obsolete"]);
    }
}

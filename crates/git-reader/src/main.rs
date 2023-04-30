// #![allow(unused_imports)]

use std::collections::HashMap;
use std::error::Error;
use std::path::Path;
use git2::{BranchType, Commit, Oid, Repository};
use git2::ObjectType::Blob;
use similar::{ChangeTag, TextDiff};
use similar::utils::TextDiffRemapper;
use smallvec::{SmallVec, smallvec};
use indicatif::ProgressBar;

use diamond_types::list::*;
use diamond_types::list::encoding::ENCODE_FULL;

fn main() -> Result<(), Box<dyn Error>> {
    // TODO: Just take this as a program argument.
    // let repo = Repository::open("/home/seph/src/diamond-types")?;
    // let file = "crates/git-reader/src/main.rs";
    // let branch = "master";

    let repo = Repository::open("/home/seph/3rdparty/node")?;
    let file = "src/node.cc";
    let branch = "master";

    // let repo = Repository::open("/home/seph/3rdparty/linux")?;
    // let file = "drivers/gpu/drm/i915/intel_display.c";

    // let repo = Repository::open("/home/seph/3rdparty/git")?;
    // let file = "Makefile";
    // let branch = "master";

    // let repo = Repository::open("/home/seph/3rdparty/yjs")?;
    // let file = "package.json";
    // let file = "y.js";

    // let repo = Repository::open("/home/seph/temp/g")?;
    // let file = "foo";

    let path = Path::new(file);

    println!("Loading {:?} from {:?}", path, repo.path());

    // let head = repo.head().unwrap();
    let head = repo.find_branch(branch, BranchType::Local).unwrap().into_reference();

    // let y = head.resolve().unwrap();
    // dbg!(&head.name(), head.target());


    let mut scan_frontier = Vec::new();
    let mut fwd_frontier = Vec::new();

    // Could wrap this stuff up in a struct or something, but its not a big deal.
    // let mut commits_seen = HashSet::new();
    let mut commit_children = HashMap::<Oid, SmallVec<[Oid; 3]>>::new();
    let mut commit_parents = HashMap::<Oid, SmallVec<[Oid; 3]>>::new();

    // (parents, children).
    // let mut commit_info = HashMap::<Oid, (SmallVec<[Oid; 3]>, SmallVec<[Oid; 3]>)>::new();

    let c = head.peel_to_commit().unwrap();
    // dbg!(c.id());
    scan_frontier.push(c.id());
    // Mark the final change as having no children.
    commit_children.insert(c.id(), smallvec![]);

    let start = std::time::SystemTime::now();

    println!("Scanning frontier...");
    while let Some(c_id) = scan_frontier.pop() {
        // println!("cc: {} / cp: {} / sf {} / ff {}", commit_children.len(), commit_parents.len(), scan_frontier.len(), fwd_frontier.len());
        if commit_parents.contains_key(&c_id) { continue; }

        // println!("Scanning {:?}", c);

        let commit = repo.find_commit(c_id)?;

        commit_parents.insert(c_id, commit.parents().map(|p| p.id()).collect());
        for p in commit.parents() {
            let p_id = p.id();
            // dbg!(&p_id);
            scan_frontier.push(p_id);

            commit_children.entry(p_id).or_insert_with(|| SmallVec::new())
                .push(c_id);
        }

        if commit.parent_count() == 0 {
            fwd_frontier.push(commit.id());
        }
    }

    drop(scan_frontier);

    let scan_commits_time = std::time::SystemTime::now();

    println!("Scanning commits...");
    let mut oplog = ListOpLog::new();
    // let empty_branch = Branch::new();
    let mut branch_at_oid = HashMap::<Oid, (ListBranch, usize)>::new();
    // let mut branch_at_oid = HashMap::<Oid, ListBranch>::new();

    let take = |branch_at_oid: &mut HashMap<Oid, (ListBranch, usize)>, p_id: Oid| -> ListBranch {
        let (branch_here, num_children) = branch_at_oid.get_mut(&p_id).unwrap();

        debug_assert!(*num_children >= 1);
        if *num_children == 1 {
            let (branch, _) = branch_at_oid.remove(&p_id).unwrap();
            branch
        } else {
            *num_children -= 1;
            branch_here.clone()
        }
    };

    let take_branch = |branch_at_oid: &mut HashMap<Oid, (ListBranch, usize)>, oplog: &ListOpLog, commit: &Commit| -> ListBranch {
        if commit.parent_count() == 0 {
            // The branch is fresh at ROOT.
            ListBranch::new()
        } else {
            // So we need 2 things:
            // - A starting branch
            // - The desired version (which is the commit version, converted to a DT frontier).

            // TODO: This code (alternately) takes the first branch which has no other children, but
            // it ends up slower in practice because cloning a branch is cheap, but scanning git
            // commits is expensive. Go figure!

            // let mut branch = None;
            // let mut frontier: SmallVec<[LV; 2]> = smallvec![];
            // for p_id in commit.parent_ids() {
            //     let (branch_here, num_children) = branch_at_oid.get_mut(&p_id).unwrap();
            //
            //     frontier.extend_from_slice(branch_here.local_frontier_ref());
            //
            //     debug_assert!(*num_children > 0);
            //     if *num_children == 1 {
            //         let (branch_here, _) = branch_at_oid.remove(&p_id).unwrap();
            //         if branch.is_none() {
            //             branch = Some(branch_here);
            //         }
            //     } else {
            //         *num_children -= 1;
            //     }
            // }
            //
            // // The frontier might contain repeated elements. Simplify!
            // frontier.sort_unstable();
            // let merge_frontier = oplog.cg.graph.find_dominators(&frontier);
            //
            // let mut branch = branch.unwrap_or_else(|| {
            //     // We might not have found any branch with no parents.
            //     let p_id = commit.parent_id(0).unwrap();
            //     branch_at_oid[&p_id].0.clone()
            // });
            //
            // branch.merge(&oplog, merge_frontier.as_ref());
            // branch

            // Go through again and make a branch here.
            let mut iter = commit.parent_ids();
            let first_parent = iter.next().unwrap();
            let mut branch = take(branch_at_oid, first_parent);

            for p in iter {
                let child_branch = take(branch_at_oid, p);
                let frontier = child_branch.local_frontier_ref();
                branch.merge(&oplog, frontier);
            }

            branch
        }
    };

    // let mut log = std::io::BufWriter::new(std::fs::File::create("git-reader.log").unwrap());

    let bar = ProgressBar::new(commit_parents.len() as _);
    // let mut i = 0;
    while let Some(commit_id) = fwd_frontier.pop() {
        bar.inc(1);
        // if i % 1000 == 0 { println!("{i}..."); }
        // i += 1;

        // write!(log, "Pop {:?}. ({} remaining)\n", commit_id, fwd_frontier.len()).unwrap();
        // println!("Pop {:?}. ({} remaining) (bao: {})", commit_id, fwd_frontier.len(), branch_at_oid.len());

        // For something to enter fwd_frontier we must have processed all of its parents.
        let commit = repo.find_commit(commit_id)?;

        let mut branch = take_branch(&mut branch_at_oid, &oplog, &commit);

        let tree = commit.tree()?;

        // if let Some(entry) = tree.get_name(file) {
        if let Ok(entry) = tree.get_path(path) {
            // dbg!(&entry.name(), entry.kind());
            if entry.kind() == Some(Blob) {
                // println!("Processing {:?} at frontier {:?}", commit_id, &branch.frontier);
                let obj = entry.to_object(&repo)?;
                let blob = obj.as_blob().unwrap();
                let new = std::str::from_utf8(blob.content())?;

                if branch.content() != new {
                    // branch.to_owned();
                    let sig = commit.author();
                    let author = sig.name().unwrap_or("unknown");
                    let agent = oplog.get_or_create_agent_id(author);

                    let branch_string = branch.content().to_string();
                    let old = branch_string.as_str();
                    let diff = TextDiff::from_chars(old, new);
                    // I could just consume diff.ops() directly here - but that would be awkward
                    // without the string utilities.
                    // dbg!(diff.ops());

                    let remapper = TextDiffRemapper::from_text_diff(&diff, old, new);
                    // .collect::<Vec<_>>();
                    // dbg!(changes);
                    // for change in diff.iter

                    let mut pos = 0;
                    for (tag, str) in diff.ops().iter()
                        .flat_map(move |x| remapper.iter_slices(x)) {
                        // dbg!(tag, str);
                        let len = str.chars().count();
                        // dbg!((tag, str, len));
                        match tag {
                            ChangeTag::Equal => pos += len,
                            ChangeTag::Delete => {
                                branch.delete(&mut oplog, agent, pos .. pos+len);

                                // let op = branch.make_delete_op(pos, len);
                                // apply_local_operation(&mut oplog, &mut branch, agent, &[op]);
                            }
                            ChangeTag::Insert => {
                                branch.insert(&mut oplog, agent, pos, str);
                                // local_insert(&mut oplog, &mut branch, agent, pos, str);
                                pos += len;
                            }
                        }
                    }

                    assert_eq!(branch.content(), new);
                    // println!("branch '{}' -> '{}'", old, branch.content);
                } else {
                    // println!("Branch content matches expected: '{}'", branch.content);
                }
            }
        }


        let children = commit_children.get(&commit_id).unwrap();
        branch_at_oid.insert(commit_id, (branch, children.len()));

        // Go through all the children. Add any child which has all its dependencies met to the
        // frontier set.
        for c in children {
            if !branch_at_oid.contains_key(c) {
                let processed_all = commit_parents.get(c).unwrap().iter()
                    .all(|p_id| branch_at_oid.contains_key(p_id));
                if processed_all {
                    // println!("Adding {:?} to children", c);
                    fwd_frontier.push(*c);
                }
            }
        }
    }
    bar.finish();

    let end_time = std::time::SystemTime::now();

    // dbg!(&oplog);
    let branch = ListBranch::new_at_tip(&oplog);
    // println!("{}: '{}'", file, branch.content);
    println!("Branch at {:?}", branch.local_frontier_ref());

    // dbg!(&oplog.history.entries.len());
    // println!("Number of entries in history: {}", &oplog.history.num_entries());

    let data = oplog.encode(ENCODE_FULL);
    std::fs::write("data.dt", data.as_slice()).unwrap();
    println!("{} bytes written to 'data.dt'", data.len());

    // let data_old = oplog.encode_simple(EncodeOptions::default());
    // println!("(vs {} bytes)", data_old.len());

    // oplog.make_time_dag_graph("git-makefile.svg");

    let pass_1_dur = scan_commits_time.duration_since(start).unwrap();
    let pass_2_dur = end_time.duration_since(scan_commits_time).unwrap();
    let total_dur = end_time.duration_since(start).unwrap();
    println!("Time for first pass: {:?} / scan commits: {:?} / total: {:?}", pass_1_dur, pass_2_dur, total_dur);

    Ok(())
}

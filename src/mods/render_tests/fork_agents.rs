//! FORK(agents): the agents list's fork sorts, task line and name
//! (`defaults/mods/agents/fork.js`, cameronsjo/herdr-app#8), drawn by the
//! shipped mod against upstream's populated fixture.

use super::*;

/// Draws the shipped agents mod with `agents.sort` set to `sort`, t1 named
/// `fix-login` with a task and both agents labeled `project=herdr`.
fn agents_with(cx: &mut TestAppContext, sort: &str, t1_status: &str) -> String {
    let staged = Staged::new();
    std::fs::write(
        staged.0.join("main.js"),
        format!(
            "import Real from './mods/agents/main.js'; import {{ setShared }} from 'herdr';\n\
             export default class Scenario extends Real {{ init(props, cx) {{ setShared('agents.sort', '{sort}'); super.init(props, cx); }} }}"
        ),
    )
    .unwrap();
    let mut values = fixture(true, props(), updates("current", "current"));
    for (key, list) in values.iter_mut() {
        let Some(items) = list.as_array_mut() else {
            continue;
        };
        for item in items.iter_mut().filter(|i| i["address"] == "t1") {
            if key == "terminals" {
                item["name"] = json!("fix-login");
                item["labels"] = json!({"task":"fix the login test","project":"herdr"});
            }
            if key == "terminals" || key == "agents" {
                item["status"] = json!(t1_status);
            }
        }
        for item in items
            .iter_mut()
            .filter(|i| i["address"] == "t2" && key == "terminals")
        {
            item["labels"] = json!({"project":"herdr"});
        }
    }
    describe_with_values(cx, &staged, "", values, None).unwrap()
}

#[gpui::test]
fn grouped_by_project_shows_one_header_the_task_line_and_the_name(cx: &mut TestAppContext) {
    cx.update(gpui_shell::init);
    let tree = agents_with(cx, "project", "working");
    for want in ["herdr", "fix the login test", "fix-login"] {
        assert!(tree.contains(want), "missing {want:?}: {tree}");
    }
    // Both agents sit under the one project header, not their spaces' headers,
    // and each row names its space, since the header no longer does.
    assert!(tree.contains("proj-"), "{tree}");
    assert!(!tree.contains("proj-s"), "{tree}");
    for space in ["\"Local\"", "\"Remote\""] {
        assert!(tree.contains(space), "missing {space}: {tree}");
    }
}

#[gpui::test]
fn needs_you_lists_only_blocked_agents(cx: &mut TestAppContext) {
    cx.update(gpui_shell::init);
    let none = agents_with(cx, "needs", "working");
    assert!(none.contains("No agents need you."), "{none}");
    let one = agents_with(cx, "needs", "blocked");
    assert!(one.contains("ag-t1") && !one.contains("ag-t2"), "{one}");
}

// For some reason the compiler sees functions that are only used in unittests as dead code.
#![allow(dead_code)]

use crate::error::RuntimeError;

#[test]
fn test_argument_validity() {
    // Test key format.
    require_ok( ["--map", ""]);
    require_ok( ["--map", "key"]);
    require_ok( ["--map", "rel"]);
    require_err(["--map", "quux"]);
    require_ok( ["--map", "key:a"]);
    require_err(["--map", "key:quux"]);
    require_ok( ["--map", "key:a:1"]);
    require_err(["--map", "key:a:quux"]);
    require_ok( ["--map", "key:a:1~"]);
    require_ok( ["--map", "key:a:~1"]);
    require_ok( ["--map", "key:a:1~2"]);
    require_ok( ["--map", "key:a:1..2"]);
    require_ok( ["--map", "key:a:1~2..1~2"]);

    require_ok( ["--map", "", ""]);
    require_err(["--map", "", "key"]);
    require_ok( ["--map", "", "key:a"]);
    require_ok( ["--map", "", "key:a:1"]);
    require_err(["--map", "", "key:a:1..2"]);

    require_ok( ["--map", "rel:x", "rel:x:x"]);
    require_ok( ["--map", "rel:x", "rel:x:-x"]);
    require_ok( ["--map", "rel:x", "rel:x:1.4x"]);
    require_ok( ["--map", "rel:x", "rel:x:2d"]);
    require_ok( ["--map", "rel:x", "rel:x:-d"]);
    require_ok( ["--map", "rel:x", "rel:x:x-d"]);
    require_ok( ["--map", "rel:x", "rel:x:x+1"]);
    require_ok( ["--map", "rel:x", "rel:x:1+x"]);
    require_err(["--map", "rel:x", "rel:x:xd"]);
    require_err(["--map", "rel:x:x", "rel:x:1"]);
    require_err(["--map", "rel:x:d", "rel:x:1"]);
    require_ok( ["--map", "rel:x:1", "rel:x:1"]);

    // TODO: Consider whether we want to allow or forbid the following keys.
    //require_err(["--map", "key:"]);
    //require_err(["--map", "key::"]);
    //require_err(["--map", "key:a:"]);
    
    // Test --withhold.
    require_ok( ["--hook", "key:a", "--withhold"]);
    require_err(["--hook", "abs:x", "--withhold"]);
    require_err(["--hook", "key:a:1~", "--withhold"]);
    require_err(["--hook", "", "--withhold"]);
    require_err(["--hook", "--withhold", "key"]);
    require_err(["--hook", "@foo", "--withhold", "key"]);
    require_err(["--hook", "abs:x", "--hook", "key:a", "--withhold"]);
    require_err(["--hook", "key:a", "--hook", "abs:x", "--withhold"]);
    require_err(["--hook", "key:a", "abs:x", "--withhold"]);
    require_ok( ["--hook", "key:a", "abs:x", "--withhold", "key"]);
    require_ok( ["--hook", "key:a", "key:b", "--withhold"]);
    require_err(["--hook", "key:a", "key:b:1", "--withhold"]);
    require_err(["--hook", "key:a", "key:b:1", "--withhold", "key"]);
    require_ok( ["--hook", "key:a", "key:b:1", "--withhold", "key:a"]);
    require_ok( ["--hook", "key:a", "key:b:1", "--withhold", "btn"]);
}

fn require_ok(args: impl IntoIterator<Item=impl Into<String>>) {
    try_implement(args).unwrap();
}

fn require_err(args: impl IntoIterator<Item=impl Into<String>>) {
    assert!(try_implement(args).is_err());
}

fn try_implement(args: impl IntoIterator<Item=impl Into<String>>) -> Result<crate::arguments::parser::Implementation, RuntimeError> {
    let args: Vec<String> =
        std::env::args().take(1)
        .chain(args.into_iter().map(|item| item.into()))
        .collect();

    crate::arguments::parser::implement(args)
}
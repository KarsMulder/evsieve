mod framework;
pub use framework::run_test;

#[test]
fn rudimentary_test() {
    run_test(
        "--map key:a key:b",
        "key:a:1 key:c:1 key:a:0 key:c:0",
        "key:b:1 key:c:1 key:b:0 key:c:0",
    )
}

#[test]
fn test_withhold_1() {
    run_test(
        "
        --hook key:a key:b
        --hook key:a key:c
        --withhold
        ", "
        key:a:1 key:a:0
        key:a:1 key:z:1 key:a:0 key:z:0
        key:a:1@foo key:a:1@bar key:z:1 key:a:0@foo key:z:0 key:a:0@bar
        ", "
        key:a:1 key:a:0
        key:z:1 key:a:1 key:a:0 key:z:0
        key:z:1 key:a:1@foo key:a:0@foo key:z:0 key:a:1@bar key:a:0@bar
        "
    )
}

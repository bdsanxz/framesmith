use motion::json::*;

#[test]
fn reads_values_with_their_lines() {
    let node = parse("{\n  \"a\": [1, 2.5],\n  \"b\": \"x\\ny\"\n}").unwrap();
    let Json::Object(members) = node.value else { panic!() };
    assert_eq!(members[0].key, "a");
    assert_eq!(members[0].line, 2);
    assert_eq!(members[1].line, 3);
    assert_eq!(members[1].value.value, Json::String("x\ny".into()));
    let Json::Array(items) = &members[0].value.value else { panic!() };
    assert_eq!(items[1].value, Json::Number("2.5".into()));
}

#[test]
fn t020_c2_reports_the_line_of_the_first_fault() {
    let err = parse("[\n  {\"target\": \"#a\"},\n  {\"target\": }\n]").unwrap_err();
    assert_eq!(err.line, 3);
    let err = parse("[1, 2").unwrap_err();
    assert_eq!(err.line, 1);
    assert!(parse("").is_err());
    assert!(parse("[] x").is_err());
    assert!(parse("{'a': 1}").is_err());
    assert!(parse("[1,]").is_err());
}

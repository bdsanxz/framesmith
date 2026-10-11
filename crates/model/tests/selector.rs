use model::html::parse;
use model::selector::matches_in;
use model::SelectorProblem;

const PAGE: &str = r#"<body>
<div id="stage" data-fs-composition="main">
  <h1 id="title" class="big bold">Title</h1>
  <div class="card" data-kind="a"><p class="big">one</p></div>
  <div class="card" data-kind="b"><span><p>two</p></span></div>
</div>
<p class="big">outside</p>
</body>"#;

fn tags(selector: &str) -> Vec<usize> {
    matches_in(&parse(PAGE), Some(1), selector).unwrap()
}

#[test]
fn simple_forms_match() {
    assert_eq!(tags("#title"), vec![2]);
    assert_eq!(tags("h1"), vec![2]);
    assert_eq!(tags("H1"), vec![2]);
    assert_eq!(tags(".card"), vec![3, 5]);
    assert_eq!(tags("[data-kind]"), vec![3, 5]);
    assert_eq!(tags("[data-kind=b]"), vec![5]);
    assert_eq!(tags("[data-kind=\"a\"]"), vec![3]);
    assert_eq!(tags("h1.big.bold#title"), vec![2]);
    assert_eq!(tags("h1.card"), Vec::<usize>::new());
    assert_eq!(tags("#missing"), Vec::<usize>::new());
}

#[test]
fn only_elements_inside_the_composition_are_candidates() {
    // The paragraph after the composition has the class too.
    assert_eq!(tags(".big"), vec![2, 4]);
    assert_eq!(tags("*").len(), 6);
    // The composition element itself is never a target.
    assert_eq!(tags("#stage"), Vec::<usize>::new());
    // With no composition, the whole page is searched.
    assert_eq!(matches_in(&parse(PAGE), None, ".big").unwrap(), vec![2, 4, 8]);
}

#[test]
fn combinators_and_lists_match() {
    assert_eq!(tags(".card p"), vec![4, 7]);
    assert_eq!(tags(".card > p"), vec![4]);
    assert_eq!(tags(".card>p"), vec![4]);
    assert_eq!(tags("#stage > .card > span p"), vec![7]);
    // The selector is read against the whole page.
    assert_eq!(tags("body .card > p"), vec![4]);
    assert_eq!(tags("#title, .card"), vec![2, 3, 5]);
    assert_eq!(tags("  #title ,#title "), vec![2]);
}

#[test]
fn t020_invalid_selectors_are_told_from_unsupported_ones() {
    let problem = |selector: &str| matches_in(&parse(PAGE), Some(1), selector).unwrap_err();
    for invalid in [
        "", "#", ".", "a >", "> a", "a,,b", "a,", "[x", "[x=]", "!", "a b $", "#1abc", "a > > b",
    ] {
        assert_eq!(problem(invalid), SelectorProblem::Invalid, "{invalid}");
    }
    for unsupported in [
        "a:hover",
        "p:nth-child(2)",
        "p::before",
        "h1 + p",
        "h1 ~ p",
        "[a^=b]",
        "[a~=b]",
        "[a=b i]",
        ":root",
        ".a\\:b",
    ] {
        assert_eq!(problem(unsupported), SelectorProblem::Unsupported, "{unsupported}");
    }
}

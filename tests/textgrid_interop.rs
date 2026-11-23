use parole::annotation::textgrid::{TextGrid, Tier};

#[test]
fn praat_exports_agree_and_round_trip_without_losing_annotation_data() {
    let long = TextGrid::from_bytes(include_bytes!("fixtures/praat-long.TextGrid")).unwrap();
    let short = TextGrid::from_bytes(include_bytes!("fixtures/praat-short.TextGrid")).unwrap();
    assert_eq!(long, short);
    assert_eq!((long.xmin, long.xmax, long.tiers.len()), (0.0, 2.3, 2));

    let Tier::Interval { name, intervals, .. } = &long.tiers[0] else {
        panic!("expected an interval tier");
    };
    assert_eq!(name, "words");
    assert_eq!(intervals.len(), 3);
    assert_eq!(intervals[0].end, 0.45);
    assert_eq!(intervals[0].label, r#"a "quote""#);
    assert!(intervals[1].label.is_empty());
    assert_eq!(intervals[2].start, 1.25);
    assert_eq!(intervals[2].label, "ˈɛ̃\n終 ! // # 12");

    let Tier::Point { name, points, .. } = &long.tiers[1] else {
        panic!("expected a point tier");
    };
    assert_eq!(name, "clicks");
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].label, "ˈpʰa");
    assert_eq!(points[1].time, 2.1);
    assert_eq!(points[1].label, r#"bell "2""#);
    assert_eq!(TextGrid::parse(&long.to_praat_long().unwrap()).unwrap(), long);
}

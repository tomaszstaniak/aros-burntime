use burngfx::scenes::Scene;

#[test]
fn only_the_location_moves_people() {
    for s in [Scene::Map, Scene::Inventory, Scene::Trader, Scene::Doctor, Scene::Service, Scene::Info,
              Scene::Statistics, Scene::Options, Scene::Pause, Scene::Wait] {
        assert!(!s.runs().people, "{s:?}");
    }
    assert!(Scene::Location.runs().people);
}

#[test]
fn the_clock_runs_on_the_map_in_a_place_and_in_a_room_only() {
    let clock: Vec<Scene> = [Scene::Map, Scene::Location, Scene::Inventory, Scene::Trader, Scene::Doctor,
                             Scene::Service, Scene::Info, Scene::Statistics, Scene::Options, Scene::Pause,
                             Scene::Wait]
        .into_iter()
        .filter(|s| s.runs().clock)
        .collect();
    assert_eq!(clock, vec![Scene::Map, Scene::Location, Scene::Inventory]);
    assert!(!Scene::Inventory.runs().ends_turn);
}

/// v1.2's `ImageScene.OnUpdate` (ImageScene.cs:72-82) runs its subtitles
/// and nothing else: no `World.Update`, so under a door's film the day's
/// clock stands and nobody in the place moves.
#[test]
fn a_doors_film_stops_the_world() {
    let r = Scene::Image.runs();
    assert!(!r.clock && !r.people && !r.ends_turn);
}

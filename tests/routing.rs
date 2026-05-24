use vps_rust::routing::tsp::{
    Coordinate, RouteAccount, google_maps_url, haversine, nearest_neighbor, plan_route,
};

const SAO_PAULO: Coordinate = Coordinate::new(-23.5505, -46.6333);
const RIO_JANEIRO: Coordinate = Coordinate::new(-22.9068, -43.1729);
const BELO_HORIZONTE: Coordinate = Coordinate::new(-19.9167, -43.9345);
const CURITIBA: Coordinate = Coordinate::new(-25.4284, -49.2733);

#[test]
fn haversine_same_position_is_zero() {
    assert_eq!(haversine(SAO_PAULO, SAO_PAULO), 0.0);
}

#[test]
fn haversine_sp_rj_is_approximate() {
    let distance = haversine(SAO_PAULO, RIO_JANEIRO);

    assert!(
        (340.0..380.0).contains(&distance),
        "distância SP-RJ inesperada: {distance}"
    );
}

#[test]
fn haversine_is_symmetric() {
    let left = haversine(SAO_PAULO, RIO_JANEIRO);
    let right = haversine(RIO_JANEIRO, SAO_PAULO);

    assert!((left - right).abs() < 1e-9);
}

#[test]
fn nearest_neighbor_single_point() {
    assert_eq!(nearest_neighbor(&[SAO_PAULO], 0), vec![0]);
}

#[test]
fn nearest_neighbor_two_points() {
    let order = nearest_neighbor(&[SAO_PAULO, RIO_JANEIRO], 0);

    assert_eq!(order.len(), 2);
    assert!(order.contains(&0));
    assert!(order.contains(&1));
}

#[test]
fn nearest_neighbor_visits_all_points() {
    let points = [SAO_PAULO, RIO_JANEIRO, BELO_HORIZONTE, CURITIBA];
    let mut order = nearest_neighbor(&points, 0);

    order.sort_unstable();
    assert_eq!(order, vec![0, 1, 2, 3]);
}

#[test]
fn nearest_neighbor_does_not_choose_bh_second_from_sp() {
    let points = [SAO_PAULO, RIO_JANEIRO, BELO_HORIZONTE, CURITIBA];
    let order = nearest_neighbor(&points, 0);

    assert_ne!(
        order[1], 2,
        "BH não deve ser o segundo da rota saindo de SP"
    );
}

#[test]
fn google_maps_url_empty() {
    assert_eq!(google_maps_url(&[]), "");
}

#[test]
fn google_maps_url_single_point() {
    let url = google_maps_url(&[("SP".to_string(), SAO_PAULO)]);

    assert!(url.contains("google.com/maps"));
    assert!(url.contains("-23.5505"));
}

#[test]
fn google_maps_url_multiple_points() {
    let waypoints = [
        ("SP".to_string(), SAO_PAULO),
        ("RJ".to_string(), RIO_JANEIRO),
        ("BH".to_string(), BELO_HORIZONTE),
    ];
    let url = google_maps_url(&waypoints);

    assert!(url.contains("google.com/maps/dir"));
    assert!(url.contains("-23.5505"));
    assert!(url.contains("-22.9068"));
}

#[test]
fn plan_route_empty_data() {
    let result = plan_route(&[], &[]);

    assert!(result.ordered_accounts.is_empty());
    assert_eq!(result.maps_url, "");
    assert_eq!(result.total_distance_km, 0.0);
}

#[test]
fn plan_route_returns_all_accounts() {
    let accounts = accounts(&["Cliente SP", "Cliente RJ", "Cliente BH"]);
    let coords = [SAO_PAULO, RIO_JANEIRO, BELO_HORIZONTE];
    let result = plan_route(&accounts, &coords);

    assert_eq!(result.ordered_accounts.len(), 3);
    assert!(result.total_distance_km > 0.0);
    assert!(result.maps_url.contains("google.com/maps"));
}

#[test]
fn plan_route_total_distance_is_positive() {
    let accounts = accounts(&["C0", "C1", "C2", "C3"]);
    let coords = [SAO_PAULO, RIO_JANEIRO, BELO_HORIZONTE, CURITIBA];
    let result = plan_route(&accounts, &coords);

    assert!(result.total_distance_km > 100.0);
}

fn accounts(names: &[&str]) -> Vec<RouteAccount> {
    names
        .iter()
        .map(|name| RouteAccount {
            name: (*name).to_string(),
        })
        .collect()
}

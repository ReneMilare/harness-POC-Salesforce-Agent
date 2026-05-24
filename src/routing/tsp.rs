#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coordinate {
    pub lat: f64,
    pub lon: f64,
}

impl Coordinate {
    pub const fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RouteAccount {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoutePlan {
    pub ordered_accounts: Vec<RouteAccount>,
    pub maps_url: String,
    pub total_distance_km: f64,
}

pub fn haversine(a: Coordinate, b: Coordinate) -> f64 {
    let radius_km = 6371.0_f64;
    let lat1 = a.lat.to_radians();
    let lon1 = a.lon.to_radians();
    let lat2 = b.lat.to_radians();
    let lon2 = b.lon.to_radians();
    let dlat = lat2 - lat1;
    let dlon = lon2 - lon1;

    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);

    radius_km * 2.0 * h.sqrt().asin()
}

pub fn nearest_neighbor(points: &[Coordinate], start_index: usize) -> Vec<usize> {
    let len = points.len();
    if len == 0 || start_index >= len {
        return Vec::new();
    }

    let mut visited = vec![false; len];
    let mut order = vec![start_index];
    visited[start_index] = true;

    for _ in 0..(len - 1) {
        let current = *order.last().expect("order always has a current point");
        let nearest = (0..len)
            .filter(|index| !visited[*index])
            .min_by(|left, right| {
                haversine(points[current], points[*left])
                    .total_cmp(&haversine(points[current], points[*right]))
            })
            .expect("there is at least one unvisited point");

        order.push(nearest);
        visited[nearest] = true;
    }

    order
}

pub fn google_maps_url(waypoints: &[(String, Coordinate)]) -> String {
    if waypoints.is_empty() {
        return String::new();
    }

    let coords = waypoints
        .iter()
        .map(|(_, coord)| format!("{},{}", coord.lat, coord.lon))
        .collect::<Vec<_>>();

    if coords.len() == 1 {
        return format!(
            "https://www.google.com/maps/search/?api=1&query={}",
            coords[0]
        );
    }

    let origin = &coords[0];
    let destination = coords.last().expect("coords has at least two points");
    let via = coords[1..coords.len() - 1].join("|");

    let mut url = format!("https://www.google.com/maps/dir/{origin}/{destination}");
    if !via.is_empty() {
        url.push_str("?waypoints=");
        url.push_str(&percent_encode(&via));
    }

    url
}

pub fn plan_route(accounts: &[RouteAccount], coords: &[Coordinate]) -> RoutePlan {
    if accounts.is_empty() || coords.is_empty() {
        return RoutePlan {
            ordered_accounts: Vec::new(),
            maps_url: String::new(),
            total_distance_km: 0.0,
        };
    }

    let usable_len = accounts.len().min(coords.len());
    let accounts = &accounts[..usable_len];
    let coords = &coords[..usable_len];

    let order = nearest_neighbor(coords, 0);
    let ordered_accounts = order
        .iter()
        .map(|index| accounts[*index].clone())
        .collect::<Vec<_>>();
    let ordered_coords = order.iter().map(|index| coords[*index]).collect::<Vec<_>>();

    let total_distance_km = ordered_coords
        .windows(2)
        .map(|pair| haversine(pair[0], pair[1]))
        .sum::<f64>();

    let waypoints = ordered_accounts
        .iter()
        .cloned()
        .zip(ordered_coords)
        .map(|(account, coord)| (account.name, coord))
        .collect::<Vec<_>>();

    RoutePlan {
        ordered_accounts,
        maps_url: google_maps_url(&waypoints),
        total_distance_km: round_one_decimal(total_distance_km),
    }
}

fn round_one_decimal(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();

    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }

    encoded
}

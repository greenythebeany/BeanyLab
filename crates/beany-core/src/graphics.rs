//! Figure/axes/series state. The engine records what to draw; the UI renders it.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct LineStyle {
    pub color: Option<String>,
    pub line: Option<String>,
    pub marker: Option<String>,
    pub width: Option<f64>,
    pub marker_size: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Series {
    Line {
        x: Vec<f64>,
        y: Vec<f64>,
        z: Option<Vec<f64>>,
        style: LineStyle,
        name: Option<String>,
    },
    Scatter {
        x: Vec<f64>,
        y: Vec<f64>,
        z: Option<Vec<f64>>,
        size: Option<Vec<f64>>,
        color: Option<String>,
        filled: bool,
        name: Option<String>,
    },
    Bar {
        x: Vec<f64>,
        y: Vec<f64>,
        horizontal: bool,
        color: Option<String>,
        width: f64,
        name: Option<String>,
    },
    Histogram {
        data: Vec<f64>,
        bins: Option<usize>,
        color: Option<String>,
        name: Option<String>,
    },
    Stem {
        x: Vec<f64>,
        y: Vec<f64>,
        style: LineStyle,
        name: Option<String>,
    },
    Stairs {
        x: Vec<f64>,
        y: Vec<f64>,
        style: LineStyle,
        name: Option<String>,
    },
    Area {
        x: Vec<f64>,
        y: Vec<f64>,
        color: Option<String>,
        name: Option<String>,
    },
    Surface {
        x: Vec<Vec<f64>>,
        y: Vec<Vec<f64>>,
        z: Vec<Vec<f64>>,
        mesh: bool,
        name: Option<String>,
    },
    Contour {
        x: Vec<f64>,
        y: Vec<f64>,
        z: Vec<Vec<f64>>,
        levels: Option<usize>,
        filled: bool,
    },
    Image {
        z: Vec<Vec<f64>>,
        x: Option<(f64, f64)>,
        y: Option<(f64, f64)>,
    },
    Errorbar {
        x: Vec<f64>,
        y: Vec<f64>,
        err: Vec<f64>,
        style: LineStyle,
        name: Option<String>,
    },
    Pie {
        values: Vec<f64>,
        labels: Vec<String>,
    },
    Text {
        x: f64,
        y: f64,
        text: String,
        size: Option<f64>,
        bold: bool,
        latex: bool,
    },
}

/// A data table drawn inside a figure (`uitable`). Position is normalized [x y w h].
#[derive(Debug, Clone, Serialize)]
pub struct UiTable {
    pub cells: Vec<Vec<String>>,
    pub columns: Vec<String>,
    pub rows: Vec<String>,
    pub position: [f64; 4],
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Axes {
    /// Subplot grid (rows, cols, 1-based positions covered).
    pub grid: (usize, usize),
    pub cells: Vec<usize>,
    pub series: Vec<Series>,
    pub title: Option<String>,
    pub xlabel: Option<String>,
    pub ylabel: Option<String>,
    pub zlabel: Option<String>,
    pub legend: Option<Vec<String>>,
    pub legend_visible: bool,
    pub legend_location: Option<String>,
    pub grid_on: bool,
    pub box_on: bool,
    pub hold: bool,
    pub xlim: Option<(f64, f64)>,
    pub ylim: Option<(f64, f64)>,
    pub zlim: Option<(f64, f64)>,
    pub axis_equal: bool,
    pub axis_off: bool,
    pub xscale_log: bool,
    pub yscale_log: bool,
    pub colorbar: bool,
    pub colormap: Option<String>,
    pub view3d: bool,
    pub color_index: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Figure {
    pub id: u32,
    pub name: Option<String>,
    pub axes: Vec<Axes>,
    pub current_axes: usize,
    pub visible: bool,
    /// Grid of a tiled layout and the last tile used by `nexttile`.
    pub tiles: Option<(usize, usize)>,
    pub next_tile: usize,
    /// Normalized region [x y w h] that the axes occupy (rest is free for uitables).
    pub area: Option<[f64; 4]>,
    pub uitables: Vec<UiTable>,
    /// Requested window size in pixels.
    pub size: Option<(f64, f64)>,
    /// Drawn by a simulator Scope block: the UI shows a time cursor with values.
    pub scope: bool,
}

impl Figure {
    pub fn new(id: u32) -> Self {
        Figure {
            id,
            name: None,
            axes: Vec::new(),
            current_axes: 0,
            visible: true,
            tiles: None,
            next_tile: 0,
            area: None,
            uitables: Vec::new(),
            size: None,
            scope: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct Graphics {
    pub figures: Vec<Figure>,
    pub current: Option<u32>,
    /// Figures changed since the UI last pulled updates.
    pub dirty: Vec<u32>,
    pub closed: Vec<u32>,
}

impl Graphics {
    pub fn mark(&mut self, id: u32) {
        if !self.dirty.contains(&id) {
            self.dirty.push(id);
        }
    }

    pub fn figure(&mut self, id: Option<u32>) -> u32 {
        let id = id.unwrap_or_else(|| {
            let mut n = 1;
            while self.figures.iter().any(|f| f.id == n) {
                n += 1;
            }
            n
        });
        if !self.figures.iter().any(|f| f.id == id) {
            self.figures.push(Figure::new(id));
        }
        self.closed.retain(|c| *c != id);
        self.current = Some(id);
        self.mark(id);
        id
    }

    pub fn current_figure(&mut self) -> &mut Figure {
        let id = match self.current {
            Some(id) if self.figures.iter().any(|f| f.id == id) => id,
            _ => self.figure(None),
        };
        self.mark(id);
        self.figures.iter_mut().find(|f| f.id == id).unwrap()
    }

    pub fn current_axes(&mut self) -> &mut Axes {
        let fig = self.current_figure();
        if fig.axes.is_empty() {
            fig.axes.push(Axes {
                grid: (1, 1),
                cells: vec![1],
                box_on: true,
                ..Default::default()
            });
            fig.current_axes = 0;
        }
        let i = fig.current_axes.min(fig.axes.len() - 1);
        &mut fig.axes[i]
    }

    /// Axes for a new plot: clears it unless `hold on`.
    pub fn plot_axes(&mut self) -> &mut Axes {
        let ax = self.current_axes();
        if !ax.hold {
            let grid = ax.grid;
            let cells = ax.cells.clone();
            *ax = Axes {
                grid,
                cells,
                box_on: true,
                ..Default::default()
            };
        }
        ax
    }

    pub fn subplot(&mut self, rows: usize, cols: usize, cells: Vec<usize>) {
        let fig = self.current_figure();
        // Remove axes that overlap the requested cells on a different grid.
        if let Some(i) = fig
            .axes
            .iter()
            .position(|a| a.grid == (rows, cols) && a.cells == cells)
        {
            fig.current_axes = i;
            return;
        }
        fig.axes
            .retain(|a| a.grid == (rows, cols) && !a.cells.iter().any(|c| cells.contains(c)));
        fig.axes.push(Axes {
            grid: (rows, cols),
            cells,
            box_on: true,
            ..Default::default()
        });
        fig.current_axes = fig.axes.len() - 1;
    }

    pub fn close(&mut self, id: Option<u32>) {
        match id {
            Some(id) => {
                self.figures.retain(|f| f.id != id);
                self.closed.push(id);
                if self.current == Some(id) {
                    self.current = self.figures.last().map(|f| f.id);
                }
            }
            None => {
                if let Some(id) = self.current {
                    self.close(Some(id));
                }
            }
        }
    }

    pub fn close_all(&mut self) {
        let ids: Vec<u32> = self.figures.iter().map(|f| f.id).collect();
        for id in ids {
            self.close(Some(id));
        }
    }

    pub fn take_updates(&mut self) -> (Vec<Figure>, Vec<u32>) {
        let dirty = std::mem::take(&mut self.dirty);
        let figs = self
            .figures
            .iter()
            .filter(|f| dirty.contains(&f.id))
            .cloned()
            .collect();
        let closed = std::mem::take(&mut self.closed);
        (figs, closed)
    }
}

pub const DEFAULT_COLORS: [&str; 7] = [
    "#0072BD", "#D95319", "#EDB120", "#7E2F8E", "#77AC30", "#4DBEEE", "#A2142F",
];

/// Parse a line spec like `'r--o'`.
pub fn parse_linespec(s: &str) -> Option<LineStyle> {
    let mut st = LineStyle::default();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
        if two == "--" || two == "-." {
            st.line = Some(two);
            i += 2;
            continue;
        }
        match c {
            '-' | ':' => st.line = Some(c.to_string()),
            'r' => st.color = Some("#FF0000".into()),
            'g' => st.color = Some("#00FF00".into()),
            'b' => st.color = Some("#0000FF".into()),
            'c' => st.color = Some("#00FFFF".into()),
            'm' => st.color = Some("#FF00FF".into()),
            'y' => st.color = Some("#FFFF00".into()),
            'k' => st.color = Some("#000000".into()),
            'w' => st.color = Some("#FFFFFF".into()),
            'o' | '+' | '*' | '.' | 'x' | 's' | 'd' | '^' | 'v' | '>' | '<' | 'p' | 'h' => {
                st.marker = Some(c.to_string())
            }
            _ => return None,
        }
        i += 1;
    }
    if st.marker.is_some() && st.line.is_none() {
        st.line = Some("none".into());
    }
    Some(st)
}

pub fn color_name(s: &str) -> Option<String> {
    Some(
        match s.to_lowercase().as_str() {
            "r" | "red" => "#FF0000",
            "g" | "green" => "#00FF00",
            "b" | "blue" => "#0000FF",
            "c" | "cyan" => "#00FFFF",
            "m" | "magenta" => "#FF00FF",
            "y" | "yellow" => "#FFFF00",
            "k" | "black" => "#000000",
            "w" | "white" => "#FFFFFF",
            s if s.starts_with('#') => return Some(s.to_string()),
            _ => return None,
        }
        .to_string(),
    )
}

pub fn rgb_to_hex(r: f64, g: f64, b: f64) -> String {
    let c = |x: f64| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", c(r), c(g), c(b))
}

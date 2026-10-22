//! Plotting commands. They update the figure model in `Interp::graphics`.

use super::*;
use crate::graphics::*;

pub fn register(it: &mut Interp) {
    let b = &mut it.builtins;
    b.insert("figure", figure);
    b.insert("clf", |it, _, _| {
        let fig = it.graphics.current_figure();
        fig.axes.clear();
        fig.current_axes = 0;
        fig.uitables.clear();
        fig.tiles = None;
        fig.area = None;
        none()
    });
    b.insert("close", |it, a, _| {
        match a.first() {
            None => it.graphics.close(None),
            Some(v) if v.is_text() => {
                let t = v.text()?;
                if t == "all" {
                    it.graphics.close_all();
                } else {
                    let id = it
                        .graphics
                        .figures
                        .iter()
                        .find(|f| f.name.as_deref() == Some(t.as_str()))
                        .map(|f| f.id);
                    if let Some(id) = id {
                        it.graphics.close(Some(id));
                    }
                }
            }
            Some(v) => {
                for id in v.to_num()?.data {
                    it.graphics.close(Some(id as u32));
                }
            }
        }
        it.flush_graphics();
        none()
    });
    b.insert("gcf", |it, _, _| {
        let id = it.graphics.current_figure().id;
        one(Value::num(id as f64))
    });
    b.insert("gca", |it, _, _| {
        it.graphics.current_axes();
        one(Value::num(1.0))
    });
    b.insert("subplot", subplot);
    b.insert("hold", |it, a, _| {
        let ax = it.graphics.current_axes();
        ax.hold = match a.first() {
            None => !ax.hold,
            Some(v) => match v.text()?.as_str() {
                "on" | "all" => true,
                "off" => false,
                other => return Err(merr!("Unknown hold option '{}'", other)),
            },
        };
        none()
    });
    b.insert("ishold", |it, _, _| {
        let h = it.graphics.current_axes().hold;
        one(bool_val(h))
    });
    b.insert("grid", |it, a, _| {
        let ax = it.graphics.current_axes();
        ax.grid_on = match a.first() {
            None => !ax.grid_on,
            Some(v) => matches!(v.text()?.as_str(), "on" | "minor"),
        };
        none()
    });
    b.insert("box", |it, a, _| {
        let ax = it.graphics.current_axes();
        ax.box_on = match a.first() {
            None => !ax.box_on,
            Some(v) => v.text()? == "on",
        };
        none()
    });
    b.insert("axis", axis);
    b.insert("xlim", |it, a, _| lim(it, a, 'x'));
    b.insert("ylim", |it, a, _| lim(it, a, 'y'));
    b.insert("zlim", |it, a, _| lim(it, a, 'z'));
    b.insert("title", |it, a, _| label(it, a, 't'));
    b.insert("xlabel", |it, a, _| label(it, a, 'x'));
    b.insert("ylabel", |it, a, _| label(it, a, 'y'));
    b.insert("zlabel", |it, a, _| label(it, a, 'z'));
    b.insert("sgtitle", |it, a, _| {
        let t = a.first().map(|v| v.text()).transpose()?.unwrap_or_default();
        it.graphics.current_figure().name = Some(t);
        none()
    });
    b.insert("legend", legend);
    b.insert("colorbar", |it, a, _| {
        let on = !matches!(a.first().map(|v| v.text()), Some(Ok(ref t)) if t == "off");
        it.graphics.current_axes().colorbar = on;
        none()
    });
    b.insert("colormap", |it, a, _| {
        if let Some(v) = a.first() {
            if v.is_text() {
                let name = v.text()?;
                it.graphics.current_axes().colormap = Some(name);
            }
        }
        none()
    });
    // Colormap functions return their name; `colormap(jet)` then selects that scale.
    for name in [
        "parula", "jet", "hot", "cool", "gray", "bone", "copper", "turbo", "hsv", "spring",
        "summer", "autumn", "winter", "viridis",
    ] {
        b.insert(name, colormap_fn);
    }
    b.insert("view", |it, a, _| {
        let three = match a.first() {
            Some(v) if v.numel() == 1 => v.scalar()? == 3.0,
            Some(v) => v.to_num()?.data.get(1).is_none_or(|e| *e != 90.0),
            None => true,
        };
        it.graphics.current_axes().view3d = three;
        none()
    });
    b.insert("plot", |it, a, _| line_plot(it, a, false, false, false));
    b.insert("semilogx", |it, a, _| line_plot(it, a, true, false, false));
    b.insert("semilogy", |it, a, _| line_plot(it, a, false, true, false));
    b.insert("loglog", |it, a, _| line_plot(it, a, true, true, false));
    b.insert("plot3", plot3);
    b.insert("scatter", |it, a, _| scatter(it, a, false));
    b.insert("scatter3", |it, a, _| scatter(it, a, true));
    b.insert("bar", |it, a, _| bar(it, a, false));
    b.insert("barh", |it, a, _| bar(it, a, true));
    b.insert("histogram", histogram);
    b.insert("hist", |it, a, nargout| {
        if nargout > 0 {
            let x = num_arg(&a, 0)?;
            let nb = if a.len() > 1 {
                a[1].scalar()? as usize
            } else {
                10
            };
            let lo = x.data.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = x.data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let w = (hi - lo) / nb as f64;
            let mut counts = vec![0.0; nb];
            for v in &x.data {
                let k = if w == 0.0 {
                    0
                } else {
                    (((v - lo) / w).floor() as usize).min(nb - 1)
                };
                counts[k] += 1.0;
            }
            let centers = (0..nb).map(|i| lo + w * (i as f64 + 0.5)).collect();
            return Ok(vec![Value::row(counts), Value::row(centers)]);
        }
        histogram(it, a, 0)
    });
    b.insert("stem", |it, a, _| simple_xy(it, a, "stem"));
    b.insert("stairs", |it, a, _| simple_xy(it, a, "stairs"));
    b.insert("area", |it, a, _| simple_xy(it, a, "area"));
    b.insert("errorbar", errorbar);
    b.insert("quiver", |it, a, _| quiver(it, a, false));
    b.insert("quiver3", |it, a, _| quiver(it, a, true));
    b.insert("surf", |it, a, _| surface(it, a, false));
    b.insert("mesh", |it, a, _| surface(it, a, true));
    b.insert("surfc", |it, a, _| surface(it, a, false));
    b.insert("contour", |it, a, _| contour(it, a, false));
    b.insert("contourf", |it, a, _| contour(it, a, true));
    b.insert("imagesc", image);
    b.insert("image", image);
    b.insert("imshow", image);
    b.insert("pie", |it, a, _| {
        let v = num_arg(&a, 0)?;
        let labels = if a.len() > 1 {
            cellstr_list(&a[1])?
        } else {
            (1..=v.numel()).map(|i| format!("{}", i)).collect()
        };
        let ax = it.graphics.plot_axes();
        ax.series.push(Series::Pie {
            values: v.data,
            labels,
        });
        none()
    });
    b.insert("text", |it, a, _| {
        check_args(&a, 3, 20)?;
        let x = a[0].scalar()?;
        let y = a[1].scalar()?;
        let t = match &a[2] {
            Value::Cell(_) => cellstr_list(&a[2])?.join("\n"),
            v => v.text()?,
        };
        let (mut size, mut bold, mut latex) = (None, false, false);
        let mut i = 3;
        while i + 1 < a.len() {
            match a[i].text()?.to_lowercase().as_str() {
                "fontsize" => size = Some(a[i + 1].scalar()?),
                "fontweight" => bold = a[i + 1].text()?.eq_ignore_ascii_case("bold"),
                "interpreter" => latex = a[i + 1].text()?.eq_ignore_ascii_case("latex"),
                _ => {}
            }
            i += 2;
        }
        it.graphics.current_axes().series.push(Series::Text {
            x,
            y,
            text: t,
            size,
            bold,
            latex,
        });
        none()
    });
    b.insert("fplot", fplot);
    b.insert("shg", |it, _, _| {
        it.graphics.current_figure();
        none()
    });
    b.insert("set", |_, _, _| none());
    b.insert("get", |_, _, _| {
        Err(merr!("Graphics handle properties are not supported yet."))
    });
    b.insert("saveas", |_, _, _| {
        Err(merr!(
            "saveas is not supported yet; use the figure window's export button."
        ))
    });
    b.insert("exportgraphics", |_, _, _| {
        Err(merr!(
            "exportgraphics is not supported yet; use the figure window's export button."
        ))
    });
}

/// All colormap builtins share this body; `last_builtin` says which name was called.
fn colormap_fn(it: &mut Interp, _: Vec<Value>, _: usize) -> R {
    one(Value::chars(&it.last_builtin.clone()))
}

fn figure(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let given = match a.first() {
        Some(v) if !v.is_text() => Some(v.scalar()? as u32),
        _ => None,
    };
    let id = it.graphics.figure(given);
    let mut i = if given.is_some() { 1 } else { 0 };
    while i + 1 < a.len() {
        let key = a[i].text().unwrap_or_default().to_lowercase();
        let f = it.graphics.figures.iter_mut().find(|f| f.id == id).unwrap();
        match key.as_str() {
            "name" => f.name = Some(a[i + 1].text()?),
            "position" => {
                let p = a[i + 1].to_num()?.data;
                if p.len() == 4 && p[2] > 0.0 && p[3] > 0.0 {
                    f.size = Some((p[2], p[3]));
                }
            }
            _ => {}
        }
        i += 2;
    }
    one(Value::num(id as f64))
}

fn subplot(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let (m, n, p) = if a.len() == 1 {
        let v = a[0].scalar()? as usize;
        (v / 100, (v / 10) % 10, vec![v % 10])
    } else {
        check_args(&a, 3, 3)?;
        let p: Vec<usize> = a[2].to_num()?.data.iter().map(|x| *x as usize).collect();
        (a[0].scalar()? as usize, a[1].scalar()? as usize, p)
    };
    if m == 0 || n == 0 || p.iter().any(|x| *x == 0 || *x > m * n) {
        return Err(merr!("Index exceeds number of subplots."));
    }
    it.graphics.subplot(m, n, p);
    none()
}

fn axis(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let ax = it.graphics.current_axes();
    for v in &a {
        if v.is_text() {
            match v.text()?.as_str() {
                "equal" | "image" | "square" => ax.axis_equal = true,
                "normal" | "auto" => {
                    ax.axis_equal = false;
                    ax.xlim = None;
                    ax.ylim = None;
                }
                "off" => ax.axis_off = true,
                "on" => ax.axis_off = false,
                "tight" => {}
                _ => {}
            }
        } else {
            let l = v.to_num()?.data;
            if l.len() >= 4 {
                ax.xlim = Some((l[0], l[1]));
                ax.ylim = Some((l[2], l[3]));
            }
            if l.len() >= 6 {
                ax.zlim = Some((l[4], l[5]));
            }
        }
    }
    none()
}

fn lim(it: &mut Interp, a: Vec<Value>, which: char) -> R {
    let ax = it.graphics.current_axes();
    let Some(v) = a.first() else {
        let l = match which {
            'x' => ax.xlim,
            'y' => ax.ylim,
            _ => ax.zlim,
        };
        return one(l.map_or(Value::empty(), |(p, q)| Value::row(vec![p, q])));
    };
    if v.is_text() {
        match which {
            'x' => ax.xlim = None,
            'y' => ax.ylim = None,
            _ => ax.zlim = None,
        }
        return none();
    }
    let l = v.to_num()?.data;
    if l.len() != 2 {
        return Err(merr!(
            "Limits must be a 2-element vector of increasing numeric values."
        ));
    }
    let r = Some((l[0], l[1]));
    match which {
        'x' => ax.xlim = r,
        'y' => ax.ylim = r,
        _ => ax.zlim = r,
    }
    none()
}

fn label(it: &mut Interp, a: Vec<Value>, which: char) -> R {
    let t = match a.first() {
        Some(Value::Cell(_)) => cellstr_list(&a[0])?.join("\n"),
        Some(v) => v.text()?,
        None => String::new(),
    };
    let ax = it.graphics.current_axes();
    match which {
        't' => ax.title = Some(t),
        'x' => ax.xlabel = Some(t),
        'y' => ax.ylabel = Some(t),
        _ => ax.zlabel = Some(t),
    }
    none()
}

fn legend(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let ax = it.graphics.current_axes();
    if let Some(v) = a.first() {
        if v.is_text() {
            match v.text()?.as_str() {
                "off" | "hide" => {
                    ax.legend_visible = false;
                    return none();
                }
                "show" | "on" | "boxon" | "boxoff" => {
                    ax.legend_visible = true;
                    return none();
                }
                _ => {}
            }
        }
    }
    let mut names = Vec::new();
    let mut i = 0;
    while i < a.len() {
        let v = &a[i];
        match v {
            Value::Cell(_) | Value::Str(_) => names.extend(cellstr_list(v)?),
            v if v.is_text() => {
                let t = v.text()?;
                if matches!(
                    t.to_lowercase().as_str(),
                    "location"
                        | "orientation"
                        | "autoupdate"
                        | "fontsize"
                        | "box"
                        | "numcolumns"
                        | "interpreter"
                ) {
                    // Name/value options follow the labels.
                    while i + 1 < a.len() {
                        if a[i].text()?.eq_ignore_ascii_case("location") {
                            ax.legend_location = Some(a[i + 1].text()?.to_lowercase());
                        }
                        i += 2;
                    }
                    break;
                }
                names.push(t);
            }
            _ => {}
        }
        i += 1;
    }
    if !names.is_empty() {
        ax.legend = Some(names);
    }
    ax.legend_visible = true;
    none()
}

/// Split plot arguments into (x, y, spec) groups plus name/value options.
fn plot_groups(
    a: &[Value],
) -> MResult<(
    Vec<(Option<Array<f64>>, Array<f64>, Option<LineStyle>)>,
    LineStyle,
    Option<String>,
)> {
    let mut groups = Vec::new();
    let mut opts = LineStyle::default();
    let mut display_name = None;
    let mut i = 0;
    let mut nums: Vec<Array<f64>> = Vec::new();
    let flush = |nums: &mut Vec<Array<f64>>,
                 spec: Option<LineStyle>,
                 groups: &mut Vec<(Option<Array<f64>>, Array<f64>, Option<LineStyle>)>|
     -> MResult<()> {
        match nums.len() {
            0 => {}
            1 => groups.push((None, nums.remove(0), spec)),
            _ => {
                while nums.len() >= 2 {
                    let x = nums.remove(0);
                    let y = nums.remove(0);
                    groups.push((
                        Some(x),
                        y,
                        if nums.is_empty() { spec.clone() } else { None },
                    ));
                }
                if let Some(y) = nums.pop() {
                    groups.push((None, y, spec));
                }
            }
        }
        Ok(())
    };
    while i < a.len() {
        let v = &a[i];
        if v.is_text() {
            let t = v.text()?;
            if let Some(spec) = parse_linespec(&t) {
                if !t.is_empty() && !nums.is_empty() {
                    flush(&mut nums, Some(spec), &mut groups)?;
                    i += 1;
                    continue;
                }
            }
            // Name/value option.
            let val = a
                .get(i + 1)
                .ok_or_else(|| merr!("Missing value for option '{}'", t))?;
            match t.to_lowercase().as_str() {
                "linewidth" => opts.width = Some(val.scalar()?),
                "markersize" => opts.marker_size = Some(val.scalar()?),
                "color" => opts.color = color_value(val)?,
                "linestyle" => opts.line = Some(val.text()?),
                "marker" => opts.marker = Some(val.text()?),
                "displayname" => display_name = Some(val.text()?),
                _ => {}
            }
            i += 2;
            continue;
        }
        nums.push(v.to_num()?);
        i += 1;
    }
    flush(&mut nums, None, &mut groups)?;
    Ok((groups, opts, display_name))
}

pub(crate) fn color_value(v: &Value) -> MResult<Option<String>> {
    if v.is_text() {
        return Ok(color_name(&v.text()?));
    }
    let c = v.to_num()?.data;
    if c.len() == 3 {
        return Ok(Some(rgb_to_hex(c[0], c[1], c[2])));
    }
    Ok(None)
}

fn columns_of(x: Option<&Array<f64>>, y: &Array<f64>) -> Vec<(Vec<f64>, Vec<f64>)> {
    let y_vec = y.is_vector();
    match x {
        None => {
            if y_vec {
                let n = y.numel();
                vec![((1..=n).map(|i| i as f64).collect(), y.data.clone())]
            } else {
                (0..y.cols)
                    .map(|c| {
                        (
                            (1..=y.rows).map(|i| i as f64).collect(),
                            y.data[c * y.rows..(c + 1) * y.rows].to_vec(),
                        )
                    })
                    .collect()
            }
        }
        Some(x) => {
            if y_vec && x.is_vector() {
                return vec![(x.data.clone(), y.data.clone())];
            }
            if x.is_vector() {
                let n = x.numel();
                if y.rows == n {
                    (0..y.cols)
                        .map(|c| {
                            (
                                x.data.clone(),
                                y.data[c * y.rows..(c + 1) * y.rows].to_vec(),
                            )
                        })
                        .collect()
                } else {
                    (0..y.rows)
                        .map(|r| (x.data.clone(), (0..y.cols).map(|c| *y.get(r, c)).collect()))
                        .collect()
                }
            } else {
                (0..y.cols.min(x.cols))
                    .map(|c| {
                        (
                            x.data[c * x.rows..(c + 1) * x.rows].to_vec(),
                            y.data[c * y.rows..(c + 1) * y.rows].to_vec(),
                        )
                    })
                    .collect()
            }
        }
    }
}

fn next_color(ax: &mut Axes) -> String {
    let c = DEFAULT_COLORS[ax.color_index % DEFAULT_COLORS.len()].to_string();
    ax.color_index += 1;
    c
}

fn merge_style(spec: Option<LineStyle>, opts: &LineStyle) -> LineStyle {
    let mut s = spec.unwrap_or_default();
    if opts.color.is_some() {
        s.color = opts.color.clone();
    }
    if opts.width.is_some() {
        s.width = opts.width;
    }
    if opts.marker_size.is_some() {
        s.marker_size = opts.marker_size;
    }
    if opts.line.is_some() {
        s.line = opts.line.clone();
    }
    if opts.marker.is_some() {
        s.marker = opts.marker.clone();
        if s.line.as_deref() == Some("none") && opts.line.is_none() {
            s.line = Some("-".into());
        }
    }
    s
}

fn line_plot(it: &mut Interp, a: Vec<Value>, logx: bool, logy: bool, _3d: bool) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let (groups, opts, name) = plot_groups(&a)?;
    let ax = it.graphics.plot_axes();
    ax.xscale_log |= logx;
    ax.yscale_log |= logy;
    for (x, y, spec) in groups {
        for (xs, ys) in columns_of(x.as_ref(), &y) {
            let mut st = merge_style(spec.clone(), &opts);
            if st.color.is_none() {
                st.color = Some(next_color(ax));
            }
            ax.series.push(Series::Line {
                x: xs,
                y: ys,
                z: None,
                style: st,
                name: name.clone(),
            });
        }
    }
    none()
}

fn plot3(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() < 3 {
        return Err(merr!("Not enough input arguments."));
    }
    let x = a[0].to_num()?;
    let y = a[1].to_num()?;
    let z = a[2].to_num()?;
    let (_, opts, name) = plot_groups(&a[3..])?;
    let spec = a
        .get(3)
        .filter(|v| v.is_text())
        .and_then(|v| v.text().ok())
        .and_then(|t| parse_linespec(&t));
    let ax = it.graphics.plot_axes();
    ax.view3d = true;
    let mut st = merge_style(spec, &opts);
    if st.color.is_none() {
        st.color = Some(next_color(ax));
    }
    ax.series.push(Series::Line {
        x: x.data,
        y: y.data,
        z: Some(z.data),
        style: st,
        name,
    });
    none()
}

fn scatter(it: &mut Interp, a: Vec<Value>, three: bool) -> R {
    let need = if three { 3 } else { 2 };
    if a.len() < need {
        return Err(merr!("Not enough input arguments."));
    }
    let x = a[0].to_num()?.data;
    let y = a[1].to_num()?.data;
    let z = if three {
        Some(a[2].to_num()?.data)
    } else {
        None
    };
    let mut size = None;
    let mut color = None;
    let mut filled = false;
    let mut name = None;
    let mut i = need;
    let mut positional = 0;
    while i < a.len() {
        let v = &a[i];
        if v.is_text() {
            let t = v.text()?;
            if t == "filled" {
                filled = true;
                i += 1;
                continue;
            }
            if let Some(val) = a.get(i + 1) {
                match t.to_lowercase().as_str() {
                    "displayname" => name = Some(val.text()?),
                    "markerfacecolor" | "markeredgecolor" => {
                        color = color_value(val)?;
                        filled = filled || t.to_lowercase() == "markerfacecolor";
                    }
                    _ => {
                        if let Some(c) = color_name(&t) {
                            color = Some(c);
                            i += 1;
                            continue;
                        }
                    }
                }
                i += 2;
                continue;
            }
            if let Some(c) = color_name(&t) {
                color = Some(c);
            }
            i += 1;
            continue;
        }
        let n = v.to_num()?;
        if positional == 0 {
            if !n.is_empty() {
                size = Some(if n.numel() == 1 {
                    vec![n.data[0]; x.len()]
                } else {
                    n.data
                });
            }
        } else if positional == 1 && n.numel() == 3 {
            color = Some(rgb_to_hex(n.data[0], n.data[1], n.data[2]));
        }
        positional += 1;
        i += 1;
    }
    let ax = it.graphics.plot_axes();
    if three {
        ax.view3d = true;
    }
    let color = color.or_else(|| Some(next_color(ax)));
    ax.series.push(Series::Scatter {
        x,
        y,
        z,
        size,
        color,
        filled,
        name,
    });
    none()
}

fn bar(it: &mut Interp, a: Vec<Value>, horizontal: bool) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let nums: Vec<&Value> = a.iter().take_while(|v| !v.is_text()).collect();
    let (x, y) = if nums.len() >= 2 && nums[1].numel() > 1
        || (nums.len() >= 2 && nums[0].numel() == nums[1].numel())
    {
        (Some(nums[0].to_num()?), nums[1].to_num()?)
    } else {
        (None, nums[0].to_num()?)
    };
    let width = if nums.len() == 3 || (nums.len() == 2 && nums[1].numel() == 1 && x.is_none()) {
        nums.last().unwrap().scalar()?
    } else {
        0.8
    };
    let mut color = None;
    for v in a.iter().skip(nums.len()) {
        if v.is_text() {
            if let Some(c) = color_name(&v.text()?) {
                color = Some(c);
            }
        }
    }
    let ax = it.graphics.plot_axes();
    for (xs, ys) in columns_of(x.as_ref(), &y) {
        let c = color.clone().or_else(|| Some(next_color(ax)));
        ax.series.push(Series::Bar {
            x: xs,
            y: ys,
            horizontal,
            color: c,
            width,
            name: None,
        });
    }
    none()
}

fn histogram(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let x = num_arg(&a, 0)?;
    let bins = if a.len() > 1 && !a[1].is_text() {
        Some(a[1].scalar()? as usize)
    } else {
        None
    };
    let mut color = None;
    let mut i = 1;
    while i + 1 < a.len() {
        if a[i].is_text() && a[i].text()?.eq_ignore_ascii_case("FaceColor") {
            color = color_value(&a[i + 1])?;
        }
        i += 1;
    }
    let bins = bins.or_else(|| Some(super::math::auto_bins(&x.data)));
    let ax = it.graphics.plot_axes();
    let color = color.or_else(|| Some(next_color(ax)));
    ax.series.push(Series::Histogram {
        data: x.data.into_iter().filter(|v| v.is_finite()).collect(),
        bins,
        color,
        name: None,
    });
    none()
}

fn simple_xy(it: &mut Interp, a: Vec<Value>, kind: &str) -> R {
    let (groups, opts, name) = plot_groups(&a)?;
    let ax = it.graphics.plot_axes();
    for (x, y, spec) in groups {
        for (xs, ys) in columns_of(x.as_ref(), &y) {
            let mut st = merge_style(spec.clone(), &opts);
            if st.color.is_none() {
                st.color = Some(next_color(ax));
            }
            ax.series.push(match kind {
                "stem" => Series::Stem {
                    x: xs,
                    y: ys,
                    style: st,
                    name: name.clone(),
                },
                "stairs" => Series::Stairs {
                    x: xs,
                    y: ys,
                    style: st,
                    name: name.clone(),
                },
                _ => Series::Area {
                    x: xs,
                    y: ys,
                    color: st.color,
                    name: name.clone(),
                },
            });
        }
    }
    none()
}

fn errorbar(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let nums: Vec<Array<f64>> = a
        .iter()
        .take_while(|v| !v.is_text())
        .map(|v| v.to_num())
        .collect::<MResult<_>>()?;
    let (x, y, e) = match nums.len() {
        2 => (
            (1..=nums[0].numel()).map(|i| i as f64).collect(),
            nums[0].data.clone(),
            nums[1].data.clone(),
        ),
        n if n >= 3 => (
            nums[0].data.clone(),
            nums[1].data.clone(),
            nums[2].data.clone(),
        ),
        _ => return Err(merr!("Not enough input arguments.")),
    };
    let spec = a
        .get(nums.len())
        .filter(|v| v.is_text())
        .and_then(|v| v.text().ok())
        .and_then(|t| parse_linespec(&t));
    let ax = it.graphics.plot_axes();
    let mut st = spec.unwrap_or_default();
    if st.color.is_none() {
        st.color = Some(next_color(ax));
    }
    ax.series.push(Series::Errorbar {
        x,
        y,
        err: e,
        style: st,
        name: None,
    });
    none()
}

/// Arrows drawn as one line series: each shaft and its two head strokes, separated
/// by NaN breaks.
fn quiver(it: &mut Interp, a: Vec<Value>, three: bool) -> R {
    let nums: Vec<Array<f64>> = a
        .iter()
        .take_while(|v| !v.is_text())
        .map(|v| v.to_num())
        .collect::<MResult<_>>()?;
    let n_vec = if three { 3 } else { 2 };
    let n_pos = if three { 3 } else { 2 };
    // quiver(U,V), quiver(X,Y,U,V) and quiver3(X,Y,Z,U,V,W), each with an optional scale.
    let (pos, dir, scale_arg) = if nums.len() >= n_pos + n_vec {
        (
            nums[..n_pos].to_vec(),
            nums[n_pos..n_pos + n_vec].to_vec(),
            nums.get(n_pos + n_vec).map(|s| s.data[0]),
        )
    } else if nums.len() >= n_vec && !three {
        (
            Vec::new(),
            nums[..n_vec].to_vec(),
            nums.get(n_vec).map(|s| s.data[0]),
        )
    } else {
        return Err(merr!("Not enough input arguments."));
    };
    let u = &dir[0];
    let n = u.numel();
    if dir.iter().any(|d| d.numel() != n) {
        return Err(merr!("U, V (and W) must be the same size."));
    }
    let (xs, ys) = if pos.is_empty() {
        let (xx, yy) = grid_xy(&[], u)?;
        (xx.data, yy.data)
    } else if pos[0].numel() == n && pos[1].numel() == n {
        (pos[0].data.clone(), pos[1].data.clone())
    } else {
        // Vector X and Y name the columns and rows of the grid.
        let (mut xx, mut yy) = (Vec::with_capacity(n), Vec::with_capacity(n));
        for c in 0..u.cols {
            for r in 0..u.rows {
                xx.push(*pos[0].data.get(c).unwrap_or(&(c as f64 + 1.0)));
                yy.push(*pos[1].data.get(r).unwrap_or(&(r as f64 + 1.0)));
            }
        }
        (xx, yy)
    };
    let zs = if three {
        Some(pos[2].data.clone())
    } else {
        None
    };
    if xs.len() != n || ys.len() != n || zs.as_ref().is_some_and(|z| z.len() != n) {
        return Err(merr!("X, Y (and Z) must match the size of U."));
    }
    let mut spec = None;
    let mut opts = LineStyle::default();
    let mut auto = scale_arg.is_none_or(|s| s != 0.0);
    let mut factor = scale_arg.filter(|s| *s != 0.0).unwrap_or(1.0);
    let mut head = 0.2;
    let mut name = None;
    let mut i = nums.len();
    while i < a.len() {
        let t = a[i].text()?;
        if i + 1 >= a.len()
            || (spec.is_none()
                && i == nums.len()
                && parse_linespec(&t).is_some()
                && !t.is_empty()
                && t != "filled")
        {
            if let Some(s) = parse_linespec(&t) {
                spec = Some(s);
            }
            i += 1;
            continue;
        }
        let v = &a[i + 1];
        match t.to_lowercase().as_str() {
            "color" => opts.color = color_value(v)?,
            "linewidth" => opts.width = Some(v.scalar()?),
            "linestyle" => opts.line = Some(v.text()?),
            "autoscale" => {
                auto = v.is_text() && v.text()? == "on" || !v.is_text() && v.scalar()? != 0.0
            }
            "autoscalefactor" => factor = v.scalar()?,
            "maxheadsize" => head = v.scalar()?,
            "displayname" => name = Some(v.text()?),
            _ => {}
        }
        i += 2;
    }
    let comp = |k: usize, j: usize| dir.get(k).map_or(0.0, |d| d.data[j]);
    // Auto scaling fits the longest arrow into the average grid spacing.
    let scale = if auto {
        let side = (n as f64)
            .powf(1.0 / if three { 3.0 } else { 2.0 })
            .max(1.0);
        let span = |v: &[f64]| {
            let (lo, hi) = v
                .iter()
                .filter(|e| e.is_finite())
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), e| {
                    (l.min(*e), h.max(*e))
                });
            if hi > lo {
                (hi - lo) / side
            } else {
                1.0
            }
        };
        let dx = span(&xs);
        let dy = span(&ys);
        let dz = zs.as_deref().map_or(1.0, span);
        let longest = (0..n)
            .map(|j| {
                ((comp(0, j) / dx).powi(2) + (comp(1, j) / dy).powi(2) + (comp(2, j) / dz).powi(2))
                    .sqrt()
            })
            .filter(|l| l.is_finite())
            .fold(0.0, f64::max);
        if longest > 0.0 {
            factor * 0.9 / longest
        } else {
            factor
        }
    } else {
        factor
    };
    let (mut lx, mut ly, mut lz) = (Vec::new(), Vec::new(), Vec::new());
    for j in 0..n {
        let p = [xs[j], ys[j], zs.as_ref().map_or(0.0, |z| z[j])];
        let d = [comp(0, j) * scale, comp(1, j) * scale, comp(2, j) * scale];
        let tip = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
        let mut push = |q: [f64; 3]| {
            lx.push(q[0]);
            ly.push(q[1]);
            lz.push(q[2]);
        };
        push(p);
        push(tip);
        push([f64::NAN; 3]);
        if head > 0.0 {
            // Two strokes back from the tip, about 20 degrees off the shaft.
            let (c, s) = (0.94, 0.34);
            let h = head;
            let back = [-d[0] * h, -d[1] * h];
            for sg in [1.0, -1.0] {
                push([
                    tip[0] + c * back[0] - sg * s * back[1],
                    tip[1] + sg * s * back[0] + c * back[1],
                    tip[2] - d[2] * h,
                ]);
                push(tip);
            }
            push([f64::NAN; 3]);
        }
    }
    let ax = it.graphics.plot_axes();
    if three {
        ax.view3d = true;
    }
    let mut st = merge_style(spec, &opts);
    if st.color.is_none() {
        st.color = Some(next_color(ax));
    }
    if st.line.as_deref() == Some("none") {
        st.line = Some("-".into());
    }
    st.marker = None;
    ax.series.push(Series::Line {
        x: lx,
        y: ly,
        z: if three { Some(lz) } else { None },
        style: st,
        name,
    });
    none()
}

fn to_rows(m: &Array<f64>) -> Vec<Vec<f64>> {
    (0..m.rows)
        .map(|r| (0..m.cols).map(|c| *m.get(r, c)).collect())
        .collect()
}

fn grid_xy(a: &[Value], z: &Array<f64>) -> MResult<(Array<f64>, Array<f64>)> {
    if a.len() >= 3 && !a[0].is_text() {
        let x = a[0].to_num()?;
        let y = a[1].to_num()?;
        if x.is_vector() && y.is_vector() {
            let mut xx = Array::filled(z.rows, z.cols, 0.0);
            let mut yy = Array::filled(z.rows, z.cols, 0.0);
            for r in 0..z.rows {
                for c in 0..z.cols {
                    xx.set(r, c, *x.data.get(c).unwrap_or(&(c as f64 + 1.0)));
                    yy.set(r, c, *y.data.get(r).unwrap_or(&(r as f64 + 1.0)));
                }
            }
            return Ok((xx, yy));
        }
        return Ok((x, y));
    }
    let mut xx = Array::filled(z.rows, z.cols, 0.0);
    let mut yy = Array::filled(z.rows, z.cols, 0.0);
    for r in 0..z.rows {
        for c in 0..z.cols {
            xx.set(r, c, (c + 1) as f64);
            yy.set(r, c, (r + 1) as f64);
        }
    }
    Ok((xx, yy))
}

fn surface(it: &mut Interp, a: Vec<Value>, mesh: bool) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let z = if a.len() >= 3 && !a[2].is_text() {
        a[2].to_num()?
    } else {
        a[0].to_num()?
    };
    let (x, y) = grid_xy(&a, &z)?;
    let ax = it.graphics.plot_axes();
    ax.view3d = true;
    ax.series.push(Series::Surface {
        x: to_rows(&x),
        y: to_rows(&y),
        z: to_rows(&z),
        mesh,
        name: None,
    });
    none()
}

fn contour(it: &mut Interp, a: Vec<Value>, filled: bool) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let (z, levels) = if a.len() >= 3 && !a[2].is_text() {
        (
            a[2].to_num()?,
            a.get(3)
                .filter(|v| !v.is_text())
                .map(|v| v.scalar())
                .transpose()?
                .map(|x| x as usize),
        )
    } else {
        (
            a[0].to_num()?,
            a.get(1)
                .filter(|v| !v.is_text())
                .map(|v| v.scalar())
                .transpose()?
                .map(|x| x as usize),
        )
    };
    let (x, y) = grid_xy(&a, &z)?;
    let xs: Vec<f64> = (0..z.cols).map(|c| *x.get(0, c)).collect();
    let ys: Vec<f64> = (0..z.rows).map(|r| *y.get(r, 0)).collect();
    let ax = it.graphics.plot_axes();
    ax.series.push(Series::Contour {
        x: xs,
        y: ys,
        z: to_rows(&z),
        levels,
        filled,
    });
    none()
}

fn image(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let (z, xr, yr) = if a.len() >= 3 && !a[2].is_text() {
        let x = a[0].to_num()?.data;
        let y = a[1].to_num()?.data;
        (
            a[2].to_num()?,
            Some((x[0], *x.last().unwrap())),
            Some((y[0], *y.last().unwrap())),
        )
    } else {
        (a[0].to_num()?, None, None)
    };
    let ax = it.graphics.plot_axes();
    ax.series.push(Series::Image {
        z: to_rows(&z),
        x: xr,
        y: yr,
    });
    none()
}

fn fplot(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let f = a[0].clone();
    let (lo, hi) = match a.get(1) {
        Some(v) if !v.is_text() => {
            let l = v.to_num()?.data;
            (l[0], l[1])
        }
        _ => (-5.0, 5.0),
    };
    let n = 400;
    let xs: Vec<f64> = (0..n)
        .map(|i| lo + (hi - lo) * i as f64 / (n - 1) as f64)
        .collect();
    let xv = Value::row(xs.clone());
    // Try vectorized evaluation first; fall back to pointwise.
    let ys = match it.call1(&f, vec![xv]) {
        Ok(v) if v.numel() == n => v.to_num()?.data,
        _ => {
            let mut out = Vec::with_capacity(n);
            for x in &xs {
                out.push(it.call1(&f, vec![Value::num(*x)])?.scalar()?);
            }
            out
        }
    };
    let spec = a
        .iter()
        .skip(1)
        .find(|v| v.is_text())
        .and_then(|v| v.text().ok())
        .and_then(|t| parse_linespec(&t));
    let ax = it.graphics.plot_axes();
    let mut st = spec.unwrap_or_default();
    if st.color.is_none() {
        st.color = Some(next_color(ax));
    }
    ax.series.push(Series::Line {
        x: xs,
        y: ys,
        z: None,
        style: st,
        name: None,
    });
    none()
}

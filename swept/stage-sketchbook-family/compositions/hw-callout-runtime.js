// hw family runtime + hw-callout-circle helpers, copied verbatim from the Registry component
// compositions/components/hw-callout-circle.html (installed with `hyperframes add hw-callout-circle`).
// Defines window.hwOnUpdate / hwHash / hwBoil / hwBoilPose / springEase / hwCalloutBuild / hwCalloutOn / hwCalloutOff.
// Load it once per host composition, before the timeline code that calls them.
(function () {
        /* ---- hw-boil helpers (shared family runtime — copy once per host) ---- */
        window.hwOnUpdate = function (tl, fn) {
          if (!tl.__hwRenders) {
            tl.__hwRenders = [];
            tl.eventCallback("onUpdate", function () {
              for (var i = 0; i < tl.__hwRenders.length; i++) tl.__hwRenders[i]();
            });
          }
          tl.__hwRenders.push(fn);
          fn();
        };
        window.hwHash = function (n, seed) {
          var x = Math.sin(n * 127.1 + (seed || 1) * 311.7) * 43758.5453;
          return (x - Math.floor(x)) * 2 - 1;
        };
        window.hwBoil = function (tl, target, opts) {
          opts = opts || {};
          var amp = opts.amp !== undefined ? opts.amp : 1.6;
          var rot = opts.rot !== undefined ? opts.rot : 0.5;
          var fps = opts.fps || 30;
          var drop = opts.frameDrop || 3;
          var seed = opts.seed || 1;
          var els = gsap.utils.toArray(target);
          window.hwOnUpdate(tl, function () {
            var step = Math.floor((tl.time() * fps) / drop);
            for (var i = 0; i < els.length; i++) {
              gsap.set(els[i], {
                x: window.hwHash(step * 3 + i * 97, seed) * amp,
                y: window.hwHash(step * 3 + 1 + i * 97, seed) * amp,
                rotation: (opts.baseRot || 0) + window.hwHash(step * 3 + 2 + i * 97, seed) * rot,
              });
            }
          });
        };
        // boil control: authored poses over the family-locked frameDrop.
        // Unknown pose → default + console error (bounded-controls law).
        window.hwBoilPose = function (tl, target, pose, opts) {
          opts = opts || {};
          var poses = { calm: { amp: 1.6, rot: 0.5 }, lively: { amp: 2.6, rot: 0.9 } };
          if (pose === "off") return;
          var p = poses[pose];
          if (!p) {
            console.error('hw: unknown boil pose "' + pose + '" — falling back to "calm"');
            p = poses.calm;
          }
          window.hwBoil(tl, target, { amp: p.amp, rot: p.rot, frameDrop: 3, seed: opts.seed || 1 });
        };
        /* ---- spring library (spring-lab, validated 2026-08-04 — verbatim) ---- */
        var LN1000 = Math.log(1000);
        var FEELS = {
          snappy: { zeta: 0.9, response: 0.22 },
          "heavy-settle": { zeta: 1.0, response: 0.8 },
          bouncy: { zeta: 0.5, response: 0.4 },
          wobbly: { zeta: 0.28, response: 0.5 },
        };
        window.springEase = function (opts) {
          var f = opts.feel ? FEELS[opts.feel] : opts;
          var zeta = f.zeta,
            response = f.response;
          var w0 = (2 * Math.PI) / response;
          var T = LN1000 / (zeta * w0);
          var x;
          if (zeta < 1) {
            var wd = w0 * Math.sqrt(1 - zeta * zeta);
            x = function (t) {
              return (
                1 -
                Math.exp(-zeta * w0 * t) *
                  (Math.cos(wd * t) + ((zeta * w0) / wd) * Math.sin(wd * t))
              );
            };
          } else {
            x = function (t) {
              return 1 - Math.exp(-w0 * t) * (1 + w0 * t);
            };
          }
          var xT = x(T);
          return {
            ease: function (p) {
              return p >= 1 ? 1 : x(p * T) / xT;
            },
            duration: T,
            zeta: zeta,
            response: response,
          };
        };
        window.hwWobbleEllipse = function (cx, cy, rx, ry, seed, wobblePct) {
          wobblePct = wobblePct === undefined ? 3 : wobblePct;
          var N = 14,
            pts = [];
          for (var i = 0; i < N; i++) {
            var a = (i / N) * Math.PI * 2;
            var wr = 1 + window.hwHash(i * 13 + 5, seed) * (wobblePct / 100);
            pts.push([cx + Math.cos(a) * rx * wr, cy + Math.sin(a) * ry * wr]);
          }
          var mx = (pts[0][0] + pts[N - 1][0]) / 2,
            my = (pts[0][1] + pts[N - 1][1]) / 2;
          var d = "M" + mx.toFixed(1) + " " + my.toFixed(1);
          for (var j = 0; j < N; j++) {
            var p = pts[j],
              q = pts[(j + 1) % N];
            d +=
              " Q" +
              p[0].toFixed(1) +
              " " +
              p[1].toFixed(1) +
              " " +
              ((p[0] + q[0]) / 2).toFixed(1) +
              " " +
              ((p[1] + q[1]) / 2).toFixed(1);
          }
          return d;
        };

        /* ---- stroke matrix (family runtime): plain | soft | sharp | spray ----
           strokeType is a g09 variant control. Authored constants per type;
           unknown value → default ("plain") + console error (bounded-controls
           law). sharp = micro stroke-dasharray gaps (dry marker) — draw-on
           must therefore ride a MASK clone (the visible dasharray never
           animates). spray = index-seeded deterministic dots along the path
           via hwHash: position a pure function of path parameter + seed; NO
           feTurbulence anywhere.
           Returns { draw: pathToDashAnimate, group: maskedGroupOrNull } */
        window.hwStrokeTypes = {
          plain: {},
          soft: { blur: 0.6 },
          sharp: { dash: [3.0, 1.55] }, // × stroke-width
          spray: {
            coreOpacity: 0.5,
            coreBlur: 0.4,
            density: 0.16,
            scatter: 2.6,
            rMin: 1.1,
            rMax: 2.6,
          },
        };
        window.hwStrokeApply = function (pathEl, type, opts) {
          opts = opts || {};
          var cfg = window.hwStrokeTypes[type];
          if (!cfg) {
            console.error('hw: unknown strokeType "' + type + '" — falling back to "plain"');
            type = "plain";
            cfg = window.hwStrokeTypes.plain;
          }
          var svg = pathEl.ownerSVGElement;
          var ns = "http://www.w3.org/2000/svg";
          var w = parseFloat(getComputedStyle(pathEl).strokeWidth) || 6;
          if (type === "plain") return { draw: pathEl, group: null, type: type };
          if (type === "soft") {
            pathEl.style.filter = "blur(" + cfg.blur + "px)";
            return { draw: pathEl, group: null, type: type };
          }
          // sharp + spray: wrap in a masked group, draw-on animates the mask clone
          var id =
            opts.id ||
            "hwsm-" + Math.abs(Math.round(window.hwHash((opts.seed || 1) * 13.7, 5) * 1e6));
          var defs = svg.querySelector("defs");
          if (!defs) {
            defs = document.createElementNS(ns, "defs");
            svg.insertBefore(defs, svg.firstChild);
          }
          var mask = document.createElementNS(ns, "mask");
          mask.setAttribute("id", id);
          mask.setAttribute("maskUnits", "userSpaceOnUse");
          var clone = document.createElementNS(ns, "path");
          clone.setAttribute("d", pathEl.getAttribute("d"));
          clone.setAttribute("fill", "none");
          clone.setAttribute("stroke", "#fff");
          clone.setAttribute(
            "stroke-width",
            w * (type === "spray" ? 2 + (cfg.scatter * 2) / w : 1.7),
          );
          clone.setAttribute("stroke-linecap", "round");
          clone.setAttribute("stroke-linejoin", "round");
          mask.appendChild(clone);
          defs.appendChild(mask);
          var group = document.createElementNS(ns, "g");
          group.setAttribute("mask", "url(#" + id + ")");
          pathEl.parentNode.insertBefore(group, pathEl);
          group.appendChild(pathEl);
          if (type === "sharp") {
            pathEl.setAttribute(
              "stroke-dasharray",
              (cfg.dash[0] * w).toFixed(1) + " " + (cfg.dash[1] * w).toFixed(1),
            );
          } else {
            // spray: faint core + seeded dots
            pathEl.setAttribute("stroke-opacity", cfg.coreOpacity);
            pathEl.style.filter = "blur(" + cfg.coreBlur + "px)";
            var len = pathEl.getTotalLength();
            var n = Math.round(len * cfg.density * (opts.densityScale || 1));
            var seed = opts.seed || 1;
            var dots = document.createElementNS(ns, "g");
            for (var i = 0; i < n; i++) {
              var t = Math.min(
                1,
                Math.max(0, (i + 0.5) / n + (window.hwHash(i * 7, seed) * 0.35) / n),
              );
              var pt = pathEl.getPointAtLength(t * len);
              var pt2 = pathEl.getPointAtLength(Math.min(len, t * len + 0.5));
              var tx = pt2.x - pt.x,
                ty = pt2.y - pt.y;
              var tl2 = Math.hypot(tx, ty) || 1;
              var nx = -ty / tl2,
                nyv = tx / tl2;
              var off = window.hwHash(i * 7 + 1, seed) * (cfg.scatter + w * 0.55);
              var r = cfg.rMin + Math.abs(window.hwHash(i * 7 + 2, seed)) * (cfg.rMax - cfg.rMin);
              var c = document.createElementNS(ns, "circle");
              c.setAttribute("cx", (pt.x + nx * off).toFixed(1));
              c.setAttribute("cy", (pt.y + nyv * off).toFixed(1));
              c.setAttribute("r", r.toFixed(2));
              c.setAttribute("fill", getComputedStyle(pathEl).stroke);
              dots.appendChild(c);
            }
            group.appendChild(dots);
          }
          return { draw: clone, group: group, type: type };
        };

        /* Draw-on across the stroke matrix: dash-animate whatever hwStrokeApply
           says. (The pen-velocity ease variant rides the write-on item, not
           this one.) */
        window.hwDrawOn = function (tl, applied, at, dur, opts) {
          opts = opts || {};
          var p = applied.draw;
          var len = p.getTotalLength();
          // the draw target is always solid (plain/soft: the path itself; sharp/spray:
          // the mask clone — the visible path keeps its texture dasharray untouched)
          p.setAttribute("stroke-dasharray", len + " " + len);
          p.setAttribute("stroke-dashoffset", len);
          gsap.set(p, { opacity: 0 }); // kill the round-cap start nub pre-draw
          var ease = opts.ease || "power2.inOut";
          tl.set(p, { opacity: 1 }, at);
          tl.to(
            p,
            { strokeDashoffset: 0, duration: dur === undefined ? 0.7 : dur, ease: ease },
            at,
          );
        };

        // ---- hw-callout helpers (copy these too) ----
        // Builds outline + scribble + connector + label inside the wrapper and
        // stores the draw records + control poses hwCalloutOn consumes.
        // opts — content (uncounted): scribble (bool), seed, label, connector (bool)
        //        controls (g09 — see the CONFIG block, the published-parameter
        //        list): labelAt, strokeType, boil
        window.hwCalloutBuild = function (target, opts) {
          opts = opts || {};
          var seed = opts.seed || 1;
          var el = document.querySelector(target);
          var svg = el.querySelector("svg");
          var vb = svg.getAttribute("viewBox").split(" ");
          var W = parseFloat(vb[2]),
            H = parseFloat(vb[3]);
          var cx = W / 2,
            cy = H / 2,
            rx = W * 0.42,
            ry = H * 0.36;

          var outline = el.querySelector(".hw-co-outline");
          outline.setAttribute("d", window.hwWobbleEllipse(cx, cy, rx, ry, seed, 4));

          var scr = el.querySelector(".hw-co-scribble");
          if (opts.scribble === false) {
            scr.style.display = "none";
          } else {
            /* Denser rows so the zigzag reads as a scribbled hatch. At 6 rows
               the near-horizontal runs dominate and it renders as stacked bars. */
            var d = "",
              rows = 11;
            for (var i = 0; i < rows; i++) {
              var t = (i + 0.5) / rows;
              var yy = cy - ry + t * ry * 2;
              var half = Math.sqrt(Math.max(0.05, 1 - Math.pow((yy - cy) / ry, 2)));
              var xw = rx * half * 0.86;
              var xa = cx - xw + window.hwHash(i * 7 + 2, seed) * 8;
              var xb = cx + xw + window.hwHash(i * 7 + 3, seed) * 8;
              d += (i === 0 ? "M " : " L ") + (i % 2 ? xa : xb).toFixed(1) + " " + yy.toFixed(1);
              d +=
                " L " +
                (i % 2 ? xb : xa).toFixed(1) +
                " " +
                (yy + window.hwHash(i * 7 + 4, seed) * 4).toFixed(1);
            }
            scr.setAttribute("d", d);
          }

          var conn = el.querySelector(".hw-co-connector");
          var label = el.querySelector(".hw-co-label");
          // labelAt is a declared variant control — bounded: unknown value
          // falls back to the default WITH a console error. Validated BEFORE
          // the connector branch so a malformed value always logs, even when
          // connector: false leaves the control inert.
          var at = opts.labelAt || "right";
          if (at !== "left" && at !== "right" && at !== "below") {
            console.error('hw: unknown labelAt "' + at + '" — falling back to "right"');
            at = "right";
          }
          var ex = null,
            ey = null;
          if (opts.connector === false) {
            conn.style.display = "none";
          } else {
            var sx = at === "left" ? cx - rx : at === "below" ? cx : cx + rx;
            var sy = at === "below" ? cy + ry : cy + ry * 0.35;
            ex = at === "left" ? sx - 70 : at === "below" ? sx + 40 : sx + 70;
            ey = sy + 55;
            conn.setAttribute(
              "d",
              "M " +
                sx.toFixed(1) +
                " " +
                sy.toFixed(1) +
                " Q " +
                ((sx + ex) / 2 + window.hwHash(21, seed) * 14).toFixed(1) +
                " " +
                (sy + 46).toFixed(1) +
                " " +
                ex.toFixed(1) +
                " " +
                ey.toFixed(1),
            );
            label.style.left = (at === "left" ? ex - 180 : ex + 12) + "px";
            label.style.top = ey - 24 + "px";
          }
          label.textContent = opts.label || "";

          // strokeType control: OUTLINE + CONNECTOR take the texture (the
          // scribble stays plain). Bounding lives inside hwStrokeApply.
          var strokeType = opts.strokeType || "plain";
          var idBase = (el.id || "hw-co") + "-sm";
          el.__hwCo = {
            rx: rx,
            seed: seed,
            boil: opts.boil || "calm",
            // the label pops out of the connector tip (falls back to center
            // when the connector anatomy is off)
            popOrigin: ex === null ? "50% 50%" : ex.toFixed(1) + "px " + ey.toFixed(1) + "px",
            draw: {
              outline: window.hwStrokeApply(outline, strokeType, {
                seed: seed,
                id: idBase + "-outline",
              }),
              scribble:
                opts.scribble === false ? null : window.hwStrokeApply(scr, "plain", { seed: seed }),
              connector:
                opts.connector === false
                  ? null
                  : window.hwStrokeApply(conn, strokeType, {
                      seed: seed,
                      id: idBase + "-connector",
                    }),
            },
          };
        };
        // Draw sequence: outline -> scribble -> connector -> label pop, with
        // the g21 contact squash on the shapes group at pop arrival. The boil
        // pose registers here too — the host wires a position only.
        window.hwCalloutOn = function (tl, target, at) {
          var el = document.querySelector(target);
          var co = el.__hwCo;
          var t = at;
          ["outline", "scribble", "connector"].forEach(function (k) {
            var rec = co.draw[k];
            if (!rec) return;
            var dur = k === "scribble" ? 0.55 : k === "connector" ? 0.35 : 0.7;
            window.hwDrawOn(tl, rec, t, dur);
            t += dur * 0.85;
          });
          // label pop: the pop ARRIVES WITH MOMENTUM (accelerating into
          // contact) — the squash absorbs it; a settling back.out crosses
          // scale 1.0 at ~zero speed and the computed squash is invisible
          // (measured, not hand-picked)
          var pop = el.querySelector(".hw-co-pop");
          var popEase = gsap.parseEase("power2.in");
          gsap.set(pop, { scale: 0.001, opacity: 0, transformOrigin: co.popOrigin });
          tl.to(pop, { opacity: 1, duration: 0.08, ease: "none" }, t);
          tl.to(pop, { scale: 1, duration: 0.3, ease: popEase, immediateRender: false }, t);
          // arrival velocity in px of radial (rx) travel, measured at the
          // FIRST crossing of 1.0 — a settling ease's end-slope is ~0 and is
          // NOT the arrival velocity
          var eps = 1e-4;
          var pc = 1;
          for (var k2 = 1; k2 <= 1000; k2++) {
            if (popEase(k2 / 1000) >= 1) {
              pc = k2 / 1000;
              break;
            }
          }
          var vPop =
            ((popEase(Math.min(pc + eps, 1)) - popEase(pc - eps)) / (2 * eps)) * (co.rx / 0.3);
          var S = Math.min(0.08 * (Math.abs(vPop) / 2000), 0.25); // 0.08 = light drawn line tier
          var rec2 = window.springEase({ feel: "snappy" });
          var st = { s: 0 };
          var deform = el.querySelector(".hw-co-deform");
          gsap.set(deform, { transformOrigin: "50% 100%" });
          // volume-preserving squash on the SHAPES wrapper only — the label
          // rides the uniform-scale pop OUTSIDE the deform, so non-uniform
          // scale never reaches glyphs. Read-back assertion enforces the law.
          window.hwOnUpdate(tl, function () {
            var along = 1 - st.s;
            gsap.set(deform, { scaleY: along, scaleX: 1 / along });
            var ax = parseFloat(gsap.getProperty(deform, "scaleX"));
            var ay = parseFloat(gsap.getProperty(deform, "scaleY"));
            if (Math.abs(ax * ay - 1) > 0.01)
              console.error("hw volume law violated (callout): " + (ax * ay).toFixed(4));
          });
          // contact: squash in 2 frames, 1-frame hold, spring home (factory
          // duration consumed verbatim)
          tl.to(st, { s: S, duration: 2 / 30, ease: "power3.out" }, t + 0.3);
          tl.to(st, { s: 0, duration: rec2.duration, ease: rec2.ease }, t + 0.3 + 3 / 30);
          // boil control (family base) — x/y/rotation on the boil wrapper only
          window.hwBoilPose(tl, el.querySelector(".hw-co-boil"), co.boil, { seed: co.seed });
        };
        window.hwCalloutOff = function (tl, target, at) {
          tl.to(target, { opacity: 0, duration: 0.35, ease: "power2.in" }, at);
        };

})();

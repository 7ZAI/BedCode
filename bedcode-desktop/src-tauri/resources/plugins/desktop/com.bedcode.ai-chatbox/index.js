/**
* @vue/shared v3.5.39
* (c) 2018-present Yuxi (Evan) You and Vue contributors
* @license MIT
**/
// @__NO_SIDE_EFFECTS__
function Tn(e) {
  const t = /* @__PURE__ */ Object.create(null);
  for (const n of e.split(",")) t[n] = 1;
  return (n) => n in t;
}
const Oe = process.env.NODE_ENV !== "production" ? Object.freeze({}) : {}, $n = process.env.NODE_ENV !== "production" ? Object.freeze([]) : [], he = () => {
}, Pn = (e) => e.charCodeAt(0) === 111 && e.charCodeAt(1) === 110 && // uppercase letter
(e.charCodeAt(2) > 122 || e.charCodeAt(2) < 97), Mn = (e) => e.startsWith("onUpdate:"), J = Object.assign, zn = Object.prototype.hasOwnProperty, ct = (e, t) => zn.call(e, t), E = Array.isArray, ie = (e) => Ge(e) === "[object Map]", Kt = (e) => Ge(e) === "[object Set]", V = (e) => typeof e == "function", F = (e) => typeof e == "string", ue = (e) => typeof e == "symbol", A = (e) => e !== null && typeof e == "object", jn = (e) => (A(e) || V(e)) && V(e.then) && V(e.catch), Ht = Object.prototype.toString, Ge = (e) => Ht.call(e), Ft = (e) => Ge(e).slice(8, -1), Ut = (e) => Ge(e) === "[object Object]", bt = (e) => F(e) && e !== "NaN" && e[0] !== "-" && "" + parseInt(e, 10) === e, yt = (e) => {
  const t = /* @__PURE__ */ Object.create(null);
  return (n) => t[n] || (t[n] = e(n));
}, Kn = /\B([A-Z])/g, Hn = yt(
  (e) => e.replace(Kn, "-$1").toLowerCase()
), Lt = yt((e) => e.charAt(0).toUpperCase() + e.slice(1)), Fn = yt(
  (e) => e ? `on${Lt(e)}` : ""
), B = (e, t) => !Object.is(e, t), Un = (e, ...t) => {
  for (let n = 0; n < e.length; n++)
    e[n](...t);
}, Wt = (e) => {
  const t = parseFloat(e);
  return isNaN(t) ? e : t;
};
let Vt;
const Qe = () => Vt || (Vt = typeof globalThis < "u" ? globalThis : typeof self < "u" ? self : typeof window < "u" ? window : typeof global < "u" ? global : {});
function _t(e) {
  if (E(e)) {
    const t = {};
    for (let n = 0; n < e.length; n++) {
      const r = e[n], s = F(r) ? Jn(r) : _t(r);
      if (s)
        for (const o in s)
          t[o] = s[o];
    }
    return t;
  } else if (F(e) || A(e))
    return e;
}
const Ln = /;(?![^(]*\))/g, Wn = /:([^]+)/, Bn = /\/\*[^]*?\*\//g;
function Jn(e) {
  const t = {};
  return e.replace(Bn, "").split(Ln).forEach((n) => {
    if (n) {
      const r = n.split(Wn);
      r.length > 1 && (t[r[0].trim()] = r[1].trim());
    }
  }), t;
}
function me(e) {
  let t = "";
  if (F(e))
    t = e;
  else if (E(e))
    for (let n = 0; n < e.length; n++) {
      const r = me(e[n]);
      r && (t += r + " ");
    }
  else if (A(e))
    for (const n in e)
      e[n] && (t += n + " ");
  return t.trim();
}
const Bt = (e) => !!(e && e.__v_isRef === !0), X = (e) => F(e) ? e : e == null ? "" : E(e) || A(e) && (e.toString === Ht || !V(e.toString)) ? Bt(e) ? X(e.value) : JSON.stringify(e, Jt, 2) : String(e), Jt = (e, t) => Bt(t) ? Jt(e, t.value) : ie(t) ? {
  [`Map(${t.size})`]: [...t.entries()].reduce(
    (n, [r, s], o) => (n[tt(r, o) + " =>"] = s, n),
    {}
  )
} : Kt(t) ? {
  [`Set(${t.size})`]: [...t.values()].map((n) => tt(n))
} : ue(t) ? tt(t) : A(t) && !E(t) && !Ut(t) ? String(t) : t, tt = (e, t = "") => {
  var n;
  return (
    // Symbol.description in es2019+ so we need to cast here to pass
    // the lib: es2016 check
    ue(e) ? `Symbol(${(n = e.description) != null ? n : t})` : e
  );
};
/**
* @vue/reactivity v3.5.39
* (c) 2018-present Yuxi (Evan) You and Vue contributors
* @license MIT
**/
function q(e, ...t) {
  console.warn(`[Vue warn] ${e}`, ...t);
}
let _;
const nt = /* @__PURE__ */ new WeakSet();
class qn {
  constructor(t) {
    this.fn = t, this.deps = void 0, this.depsTail = void 0, this.flags = 5, this.next = void 0, this.cleanup = void 0, this.scheduler = void 0;
  }
  pause() {
    this.flags |= 64;
  }
  resume() {
    this.flags & 64 && (this.flags &= -65, nt.has(this) && (nt.delete(this), this.trigger()));
  }
  /**
   * @internal
   */
  notify() {
    this.flags & 2 && !(this.flags & 32) || this.flags & 8 || Yt(this);
  }
  run() {
    if (!(this.flags & 1))
      return this.fn();
    this.flags |= 2, At(this), Gt(this);
    const t = _, n = H;
    _ = this, H = !0;
    try {
      return this.fn();
    } finally {
      process.env.NODE_ENV !== "production" && _ !== this && q(
        "Active effect was not restored correctly - this is likely a Vue internal bug."
      ), Qt(this), _ = t, H = n, this.flags &= -3;
    }
  }
  stop() {
    if (this.flags & 1) {
      for (let t = this.deps; t; t = t.nextDep)
        kt(t);
      this.deps = this.depsTail = void 0, At(this), this.onStop && this.onStop(), this.flags &= -2;
    }
  }
  trigger() {
    this.flags & 64 ? nt.add(this) : this.scheduler ? this.scheduler() : this.runIfDirty();
  }
  /**
   * @internal
   */
  runIfDirty() {
    ut(this) && this.run();
  }
  get dirty() {
    return ut(this);
  }
}
let qt = 0, Ne, Se;
function Yt(e, t = !1) {
  if (e.flags |= 8, t) {
    e.next = Se, Se = e;
    return;
  }
  e.next = Ne, Ne = e;
}
function xt() {
  qt++;
}
function wt() {
  if (--qt > 0)
    return;
  if (Se) {
    let t = Se;
    for (Se = void 0; t; ) {
      const n = t.next;
      t.next = void 0, t.flags &= -9, t = n;
    }
  }
  let e;
  for (; Ne; ) {
    let t = Ne;
    for (Ne = void 0; t; ) {
      const n = t.next;
      if (t.next = void 0, t.flags &= -9, t.flags & 1)
        try {
          t.trigger();
        } catch (r) {
          e || (e = r);
        }
      t = n;
    }
  }
  if (e) throw e;
}
function Gt(e) {
  for (let t = e.deps; t; t = t.nextDep)
    t.version = -1, t.prevActiveLink = t.dep.activeLink, t.dep.activeLink = t;
}
function Qt(e) {
  let t, n = e.depsTail, r = n;
  for (; r; ) {
    const s = r.prevDep;
    r.version === -1 ? (r === n && (n = s), kt(r), Yn(r)) : t = r, r.dep.activeLink = r.prevActiveLink, r.prevActiveLink = void 0, r = s;
  }
  e.deps = t, e.depsTail = n;
}
function ut(e) {
  for (let t = e.deps; t; t = t.nextDep)
    if (t.dep.version !== t.version || t.dep.computed && (Xt(t.dep.computed) || t.dep.version !== t.version))
      return !0;
  return !!e._dirty;
}
function Xt(e) {
  if (e.flags & 4 && !(e.flags & 16) || (e.flags &= -17, e.globalVersion === Ce) || (e.globalVersion = Ce, !e.isSSR && e.flags & 128 && (!e.deps && !e._dirty || !ut(e))))
    return;
  e.flags |= 2;
  const t = e.dep, n = _, r = H;
  _ = e, H = !0;
  try {
    Gt(e);
    const s = e.fn(e._value);
    (t.version === 0 || B(s, e._value)) && (e.flags |= 128, e._value = s, t.version++);
  } catch (s) {
    throw t.version++, s;
  } finally {
    _ = n, H = r, Qt(e), e.flags &= -3;
  }
}
function kt(e, t = !1) {
  const { dep: n, prevSub: r, nextSub: s } = e;
  if (r && (r.nextSub = s, e.prevSub = void 0), s && (s.prevSub = r, e.nextSub = void 0), process.env.NODE_ENV !== "production" && n.subsHead === e && (n.subsHead = s), n.subs === e && (n.subs = r, !r && n.computed)) {
    n.computed.flags &= -5;
    for (let o = n.computed.deps; o; o = o.nextDep)
      kt(o, !0);
  }
  !t && !--n.sc && n.map && n.map.delete(n.key);
}
function Yn(e) {
  const { prevDep: t, nextDep: n } = e;
  t && (t.nextDep = n, e.prevDep = void 0), n && (n.prevDep = t, e.nextDep = void 0);
}
let H = !0;
const Zt = [];
function be() {
  Zt.push(H), H = !1;
}
function ye() {
  const e = Zt.pop();
  H = e === void 0 ? !0 : e;
}
function At(e) {
  const { cleanup: t } = e;
  if (e.cleanup = void 0, t) {
    const n = _;
    _ = void 0;
    try {
      t();
    } finally {
      _ = n;
    }
  }
}
let Ce = 0;
class Gn {
  constructor(t, n) {
    this.sub = t, this.dep = n, this.version = n.version, this.nextDep = this.prevDep = this.nextSub = this.prevSub = this.prevActiveLink = void 0;
  }
}
class Et {
  // TODO isolatedDeclarations "__v_skip"
  constructor(t) {
    this.computed = t, this.version = 0, this.activeLink = void 0, this.subs = void 0, this.map = void 0, this.key = void 0, this.sc = 0, this.__v_skip = !0, process.env.NODE_ENV !== "production" && (this.subsHead = void 0);
  }
  track(t) {
    if (!_ || !H || _ === this.computed)
      return;
    let n = this.activeLink;
    if (n === void 0 || n.sub !== _)
      n = this.activeLink = new Gn(_, this), _.deps ? (n.prevDep = _.depsTail, _.depsTail.nextDep = n, _.depsTail = n) : _.deps = _.depsTail = n, en(n);
    else if (n.version === -1 && (n.version = this.version, n.nextDep)) {
      const r = n.nextDep;
      r.prevDep = n.prevDep, n.prevDep && (n.prevDep.nextDep = r), n.prevDep = _.depsTail, n.nextDep = void 0, _.depsTail.nextDep = n, _.depsTail = n, _.deps === n && (_.deps = r);
    }
    return process.env.NODE_ENV !== "production" && _.onTrack && _.onTrack(
      J(
        {
          effect: _
        },
        t
      )
    ), n;
  }
  trigger(t) {
    this.version++, Ce++, this.notify(t);
  }
  notify(t) {
    xt();
    try {
      if (process.env.NODE_ENV !== "production")
        for (let n = this.subsHead; n; n = n.nextSub)
          n.sub.onTrigger && !(n.sub.flags & 8) && n.sub.onTrigger(
            J(
              {
                effect: n.sub
              },
              t
            )
          );
      for (let n = this.subs; n; n = n.prevSub)
        n.sub.notify() && n.sub.dep.notify();
    } finally {
      wt();
    }
  }
}
function en(e) {
  if (e.dep.sc++, e.sub.flags & 4) {
    const t = e.dep.computed;
    if (t && !e.dep.subs) {
      t.flags |= 20;
      for (let r = t.deps; r; r = r.nextDep)
        en(r);
    }
    const n = e.dep.subs;
    n !== e && (e.prevSub = n, n && (n.nextSub = e)), process.env.NODE_ENV !== "production" && e.dep.subsHead === void 0 && (e.dep.subsHead = e), e.dep.subs = e;
  }
}
const dt = /* @__PURE__ */ new WeakMap(), ae = /* @__PURE__ */ Symbol(
  process.env.NODE_ENV !== "production" ? "Object iterate" : ""
), ft = /* @__PURE__ */ Symbol(
  process.env.NODE_ENV !== "production" ? "Map keys iterate" : ""
), De = /* @__PURE__ */ Symbol(
  process.env.NODE_ENV !== "production" ? "Array iterate" : ""
);
function $(e, t, n) {
  if (H && _) {
    let r = dt.get(e);
    r || dt.set(e, r = /* @__PURE__ */ new Map());
    let s = r.get(n);
    s || (r.set(n, s = new Et()), s.map = r, s.key = n), process.env.NODE_ENV !== "production" ? s.track({
      target: e,
      type: t,
      key: n
    }) : s.track();
  }
}
function te(e, t, n, r, s, o) {
  const i = dt.get(e);
  if (!i) {
    Ce++;
    return;
  }
  const l = (a) => {
    a && (process.env.NODE_ENV !== "production" ? a.trigger({
      target: e,
      type: t,
      key: n,
      newValue: r,
      oldValue: s,
      oldTarget: o
    }) : a.trigger());
  };
  if (xt(), t === "clear")
    i.forEach(l);
  else {
    const a = E(e), u = a && bt(n);
    if (a && n === "length") {
      const d = Number(r);
      i.forEach((c, f) => {
        (f === "length" || f === De || !ue(f) && f >= d) && l(c);
      });
    } else
      switch ((n !== void 0 || i.has(void 0)) && l(i.get(n)), u && l(i.get(De)), t) {
        case "add":
          a ? u && l(i.get("length")) : (l(i.get(ae)), ie(e) && l(i.get(ft)));
          break;
        case "delete":
          a || (l(i.get(ae)), ie(e) && l(i.get(ft)));
          break;
        case "set":
          ie(e) && l(i.get(ae));
          break;
      }
  }
  wt();
}
function de(e) {
  const t = /* @__PURE__ */ v(e);
  return t === e ? t : ($(t, "iterate", De), /* @__PURE__ */ T(e) ? t : t.map(L));
}
function Xe(e) {
  return $(e = /* @__PURE__ */ v(e), "iterate", De), e;
}
function W(e, t) {
  return /* @__PURE__ */ U(e) ? ve(/* @__PURE__ */ le(e) ? L(t) : t) : L(t);
}
const Qn = {
  __proto__: null,
  [Symbol.iterator]() {
    return st(this, Symbol.iterator, (e) => W(this, e));
  },
  concat(...e) {
    return de(this).concat(
      ...e.map((t) => E(t) ? de(t) : t)
    );
  },
  entries() {
    return st(this, "entries", (e) => (e[1] = W(this, e[1]), e));
  },
  every(e, t) {
    return G(this, "every", e, t, void 0, arguments);
  },
  filter(e, t) {
    return G(
      this,
      "filter",
      e,
      t,
      (n) => n.map((r) => W(this, r)),
      arguments
    );
  },
  find(e, t) {
    return G(
      this,
      "find",
      e,
      t,
      (n) => W(this, n),
      arguments
    );
  },
  findIndex(e, t) {
    return G(this, "findIndex", e, t, void 0, arguments);
  },
  findLast(e, t) {
    return G(
      this,
      "findLast",
      e,
      t,
      (n) => W(this, n),
      arguments
    );
  },
  findLastIndex(e, t) {
    return G(this, "findLastIndex", e, t, void 0, arguments);
  },
  // flat, flatMap could benefit from ARRAY_ITERATE but are not straight-forward to implement
  forEach(e, t) {
    return G(this, "forEach", e, t, void 0, arguments);
  },
  includes(...e) {
    return rt(this, "includes", e);
  },
  indexOf(...e) {
    return rt(this, "indexOf", e);
  },
  join(e) {
    return de(this).join(e);
  },
  // keys() iterator only reads `length`, no optimization required
  lastIndexOf(...e) {
    return rt(this, "lastIndexOf", e);
  },
  map(e, t) {
    return G(this, "map", e, t, void 0, arguments);
  },
  pop() {
    return xe(this, "pop");
  },
  push(...e) {
    return xe(this, "push", e);
  },
  reduce(e, ...t) {
    return Rt(this, "reduce", e, t);
  },
  reduceRight(e, ...t) {
    return Rt(this, "reduceRight", e, t);
  },
  shift() {
    return xe(this, "shift");
  },
  // slice could use ARRAY_ITERATE but also seems to beg for range tracking
  some(e, t) {
    return G(this, "some", e, t, void 0, arguments);
  },
  splice(...e) {
    return xe(this, "splice", e);
  },
  toReversed() {
    return de(this).toReversed();
  },
  toSorted(e) {
    return de(this).toSorted(e);
  },
  toSpliced(...e) {
    return de(this).toSpliced(...e);
  },
  unshift(...e) {
    return xe(this, "unshift", e);
  },
  values() {
    return st(this, "values", (e) => W(this, e));
  }
};
function st(e, t, n) {
  const r = Xe(e), s = r[t]();
  return r !== e && !/* @__PURE__ */ T(e) && (s._next = s.next, s.next = () => {
    const o = s._next();
    return o.done || (o.value = n(o.value)), o;
  }), s;
}
const Xn = Array.prototype;
function G(e, t, n, r, s, o) {
  const i = Xe(e), l = i !== e && !/* @__PURE__ */ T(e), a = i[t];
  if (a !== Xn[t]) {
    const c = a.apply(e, o);
    return l ? L(c) : c;
  }
  let u = n;
  i !== e && (l ? u = function(c, f) {
    return n.call(this, W(e, c), f, e);
  } : n.length > 2 && (u = function(c, f) {
    return n.call(this, c, f, e);
  }));
  const d = a.call(i, u, r);
  return l && s ? s(d) : d;
}
function Rt(e, t, n, r) {
  const s = Xe(e), o = s !== e && !/* @__PURE__ */ T(e);
  let i = n, l = !1;
  s !== e && (o ? (l = r.length === 0, i = function(u, d, c) {
    return l && (l = !1, u = W(e, u)), n.call(this, u, W(e, d), c, e);
  }) : n.length > 3 && (i = function(u, d, c) {
    return n.call(this, u, d, c, e);
  }));
  const a = s[t](i, ...r);
  return l ? W(e, a) : a;
}
function rt(e, t, n) {
  const r = /* @__PURE__ */ v(e);
  $(r, "iterate", De);
  const s = r[t](...n);
  return (s === -1 || s === !1) && /* @__PURE__ */ Fe(n[0]) ? (n[0] = /* @__PURE__ */ v(n[0]), r[t](...n)) : s;
}
function xe(e, t, n = []) {
  be(), xt();
  const r = (/* @__PURE__ */ v(e))[t].apply(e, n);
  return wt(), ye(), r;
}
const Zn = /* @__PURE__ */ Tn("__proto__,__v_isRef,__isVue"), tn = new Set(
  /* @__PURE__ */ Object.getOwnPropertyNames(Symbol).filter((e) => e !== "arguments" && e !== "caller").map((e) => Symbol[e]).filter(ue)
);
function es(e) {
  ue(e) || (e = String(e));
  const t = /* @__PURE__ */ v(this);
  return $(t, "has", e), t.hasOwnProperty(e);
}
class nn {
  constructor(t = !1, n = !1) {
    this._isReadonly = t, this._isShallow = n;
  }
  get(t, n, r) {
    if (n === "__v_skip") return t.__v_skip;
    const s = this._isReadonly, o = this._isShallow;
    if (n === "__v_isReactive")
      return !s;
    if (n === "__v_isReadonly")
      return s;
    if (n === "__v_isShallow")
      return o;
    if (n === "__v_raw")
      return r === (s ? o ? us : on : o ? cs : rn).get(t) || // receiver is not the reactive proxy, but has the same prototype
      // this means the receiver is a user proxy of the reactive proxy
      Object.getPrototypeOf(t) === Object.getPrototypeOf(r) ? t : void 0;
    const i = E(t);
    if (!s) {
      let a;
      if (i && (a = Qn[n]))
        return a;
      if (n === "hasOwnProperty")
        return es;
    }
    const l = Reflect.get(
      t,
      n,
      // if this is a proxy wrapping a ref, return methods using the raw ref
      // as receiver so that we don't have to call `toRaw` on the ref in all
      // its class methods
      /* @__PURE__ */ P(t) ? t : r
    );
    if ((ue(n) ? tn.has(n) : Zn(n)) || (s || $(t, "get", n), o))
      return l;
    if (/* @__PURE__ */ P(l)) {
      const a = i && bt(n) ? l : l.value;
      return s && A(a) ? /* @__PURE__ */ ht(a) : a;
    }
    return A(l) ? s ? /* @__PURE__ */ ht(l) : /* @__PURE__ */ Nt(l) : l;
  }
}
class ts extends nn {
  constructor(t = !1) {
    super(!1, t);
  }
  set(t, n, r, s) {
    let o = t[n];
    const i = E(t) && bt(n);
    if (!this._isShallow) {
      const u = /* @__PURE__ */ U(o);
      if (!/* @__PURE__ */ T(r) && !/* @__PURE__ */ U(r) && (o = /* @__PURE__ */ v(o), r = /* @__PURE__ */ v(r)), !i && /* @__PURE__ */ P(o) && !/* @__PURE__ */ P(r))
        return u ? (process.env.NODE_ENV !== "production" && q(
          `Set operation on key "${String(n)}" failed: target is readonly.`,
          t[n]
        ), !0) : (o.value = r, !0);
    }
    const l = i ? Number(n) < t.length : ct(t, n), a = Reflect.set(
      t,
      n,
      r,
      /* @__PURE__ */ P(t) ? t : s
    );
    return t === /* @__PURE__ */ v(s) && a && (l ? B(r, o) && te(t, "set", n, r, o) : te(t, "add", n, r)), a;
  }
  deleteProperty(t, n) {
    const r = ct(t, n), s = t[n], o = Reflect.deleteProperty(t, n);
    return o && r && te(t, "delete", n, void 0, s), o;
  }
  has(t, n) {
    const r = Reflect.has(t, n);
    return (!ue(n) || !tn.has(n)) && $(t, "has", n), r;
  }
  ownKeys(t) {
    return $(
      t,
      "iterate",
      E(t) ? "length" : ae
    ), Reflect.ownKeys(t);
  }
}
class ns extends nn {
  constructor(t = !1) {
    super(!0, t);
  }
  set(t, n) {
    return process.env.NODE_ENV !== "production" && q(
      `Set operation on key "${String(n)}" failed: target is readonly.`,
      t
    ), !0;
  }
  deleteProperty(t, n) {
    return process.env.NODE_ENV !== "production" && q(
      `Delete operation on key "${String(n)}" failed: target is readonly.`,
      t
    ), !0;
  }
}
const ss = /* @__PURE__ */ new ts(), rs = /* @__PURE__ */ new ns(), pt = (e) => e, Pe = (e) => Reflect.getPrototypeOf(e);
function os(e, t, n) {
  return function(...r) {
    const s = this.__v_raw, o = /* @__PURE__ */ v(s), i = ie(o), l = e === "entries" || e === Symbol.iterator && i, a = e === "keys" && i, u = s[e](...r), d = n ? pt : t ? ve : L;
    return !t && $(
      o,
      "iterate",
      a ? ft : ae
    ), J(
      // inheriting all iterator properties
      Object.create(u),
      {
        // iterator protocol
        next() {
          const { value: c, done: f } = u.next();
          return f ? { value: c, done: f } : {
            value: l ? [d(c[0]), d(c[1])] : d(c),
            done: f
          };
        }
      }
    );
  };
}
function Me(e) {
  return function(...t) {
    if (process.env.NODE_ENV !== "production") {
      const n = t[0] ? `on key "${t[0]}" ` : "";
      q(
        `${Lt(e)} operation ${n}failed: target is readonly.`,
        /* @__PURE__ */ v(this)
      );
    }
    return e === "delete" ? !1 : e === "clear" ? void 0 : this;
  };
}
function is(e, t) {
  const n = {
    get(s) {
      const o = this.__v_raw, i = /* @__PURE__ */ v(o), l = /* @__PURE__ */ v(s);
      e || (B(s, l) && $(i, "get", s), $(i, "get", l));
      const { has: a } = Pe(i), u = t ? pt : e ? ve : L;
      if (a.call(i, s))
        return u(o.get(s));
      if (a.call(i, l))
        return u(o.get(l));
      o !== i && o.get(s);
    },
    get size() {
      const s = this.__v_raw;
      return !e && $(/* @__PURE__ */ v(s), "iterate", ae), s.size;
    },
    has(s) {
      const o = this.__v_raw, i = /* @__PURE__ */ v(o), l = /* @__PURE__ */ v(s);
      return e || (B(s, l) && $(i, "has", s), $(i, "has", l)), s === l ? o.has(s) : o.has(s) || o.has(l);
    },
    forEach(s, o) {
      const i = this, l = i.__v_raw, a = /* @__PURE__ */ v(l), u = t ? pt : e ? ve : L;
      return !e && $(a, "iterate", ae), l.forEach((d, c) => s.call(o, u(d), u(c), i));
    }
  };
  return J(
    n,
    e ? {
      add: Me("add"),
      set: Me("set"),
      delete: Me("delete"),
      clear: Me("clear")
    } : {
      add(s) {
        const o = /* @__PURE__ */ v(this), i = Pe(o), l = /* @__PURE__ */ v(s), a = !t && !/* @__PURE__ */ T(s) && !/* @__PURE__ */ U(s) ? l : s;
        return i.has.call(o, a) || B(s, a) && i.has.call(o, s) || B(l, a) && i.has.call(o, l) || (o.add(a), te(o, "add", a, a)), this;
      },
      set(s, o) {
        !t && !/* @__PURE__ */ T(o) && !/* @__PURE__ */ U(o) && (o = /* @__PURE__ */ v(o));
        const i = /* @__PURE__ */ v(this), { has: l, get: a } = Pe(i);
        let u = l.call(i, s);
        u ? process.env.NODE_ENV !== "production" && Tt(i, l, s) : (s = /* @__PURE__ */ v(s), u = l.call(i, s));
        const d = a.call(i, s);
        return i.set(s, o), u ? B(o, d) && te(i, "set", s, o, d) : te(i, "add", s, o), this;
      },
      delete(s) {
        const o = /* @__PURE__ */ v(this), { has: i, get: l } = Pe(o);
        let a = i.call(o, s);
        a ? process.env.NODE_ENV !== "production" && Tt(o, i, s) : (s = /* @__PURE__ */ v(s), a = i.call(o, s));
        const u = l ? l.call(o, s) : void 0, d = o.delete(s);
        return a && te(o, "delete", s, void 0, u), d;
      },
      clear() {
        const s = /* @__PURE__ */ v(this), o = s.size !== 0, i = process.env.NODE_ENV !== "production" ? ie(s) ? new Map(s) : new Set(s) : void 0, l = s.clear();
        return o && te(
          s,
          "clear",
          void 0,
          void 0,
          i
        ), l;
      }
    }
  ), [
    "keys",
    "values",
    "entries",
    Symbol.iterator
  ].forEach((s) => {
    n[s] = os(s, e, t);
  }), n;
}
function sn(e, t) {
  const n = is(e, t);
  return (r, s, o) => s === "__v_isReactive" ? !e : s === "__v_isReadonly" ? e : s === "__v_raw" ? r : Reflect.get(
    ct(n, s) && s in r ? n : r,
    s,
    o
  );
}
const as = {
  get: /* @__PURE__ */ sn(!1, !1)
}, ls = {
  get: /* @__PURE__ */ sn(!0, !1)
};
function Tt(e, t, n) {
  const r = /* @__PURE__ */ v(n);
  if (r !== n && t.call(e, r)) {
    const s = Ft(e);
    q(
      `Reactive ${s} contains both the raw and reactive versions of the same object${s === "Map" ? " as keys" : ""}, which can lead to inconsistencies. Avoid differentiating between the raw and reactive versions of an object and only use the reactive version if possible.`
    );
  }
}
const rn = /* @__PURE__ */ new WeakMap(), cs = /* @__PURE__ */ new WeakMap(), on = /* @__PURE__ */ new WeakMap(), us = /* @__PURE__ */ new WeakMap();
function ds(e) {
  switch (e) {
    case "Object":
    case "Array":
      return 1;
    case "Map":
    case "Set":
    case "WeakMap":
    case "WeakSet":
      return 2;
    default:
      return 0;
  }
}
// @__NO_SIDE_EFFECTS__
function Nt(e) {
  return /* @__PURE__ */ U(e) ? e : an(
    e,
    !1,
    ss,
    as,
    rn
  );
}
// @__NO_SIDE_EFFECTS__
function ht(e) {
  return an(
    e,
    !0,
    rs,
    ls,
    on
  );
}
function an(e, t, n, r, s) {
  if (!A(e))
    return process.env.NODE_ENV !== "production" && q(
      `value cannot be made ${t ? "readonly" : "reactive"}: ${String(
        e
      )}`
    ), e;
  if (e.__v_raw && !(t && e.__v_isReactive) || e.__v_skip || !Object.isExtensible(e))
    return e;
  const o = s.get(e);
  if (o)
    return o;
  const i = ds(Ft(e));
  if (i === 0)
    return e;
  const l = new Proxy(
    e,
    i === 2 ? r : n
  );
  return s.set(e, l), l;
}
// @__NO_SIDE_EFFECTS__
function le(e) {
  return /* @__PURE__ */ U(e) ? /* @__PURE__ */ le(e.__v_raw) : !!(e && e.__v_isReactive);
}
// @__NO_SIDE_EFFECTS__
function U(e) {
  return !!(e && e.__v_isReadonly);
}
// @__NO_SIDE_EFFECTS__
function T(e) {
  return !!(e && e.__v_isShallow);
}
// @__NO_SIDE_EFFECTS__
function Fe(e) {
  return e ? !!e.__v_raw : !1;
}
// @__NO_SIDE_EFFECTS__
function v(e) {
  const t = e && e.__v_raw;
  return t ? /* @__PURE__ */ v(t) : e;
}
const L = (e) => A(e) ? /* @__PURE__ */ Nt(e) : e, ve = (e) => A(e) ? /* @__PURE__ */ ht(e) : e;
// @__NO_SIDE_EFFECTS__
function P(e) {
  return e ? e.__v_isRef === !0 : !1;
}
// @__NO_SIDE_EFFECTS__
function C(e) {
  return fs(e, !1);
}
function fs(e, t) {
  return /* @__PURE__ */ P(e) ? e : new ps(e, t);
}
class ps {
  constructor(t, n) {
    this.dep = new Et(), this.__v_isRef = !0, this.__v_isShallow = !1, this._rawValue = n ? t : /* @__PURE__ */ v(t), this._value = n ? t : L(t), this.__v_isShallow = n;
  }
  get value() {
    return process.env.NODE_ENV !== "production" ? this.dep.track({
      target: this,
      type: "get",
      key: "value"
    }) : this.dep.track(), this._value;
  }
  set value(t) {
    const n = this._rawValue, r = this.__v_isShallow || /* @__PURE__ */ T(t) || /* @__PURE__ */ U(t);
    t = r ? t : /* @__PURE__ */ v(t), B(t, n) && (this._rawValue = t, this._value = r ? t : L(t), process.env.NODE_ENV !== "production" ? this.dep.trigger({
      target: this,
      type: "set",
      key: "value",
      newValue: t,
      oldValue: n
    }) : this.dep.trigger());
  }
}
function k(e) {
  return /* @__PURE__ */ P(e) ? e.value : e;
}
class hs {
  constructor(t, n, r) {
    this.fn = t, this.setter = n, this._value = void 0, this.dep = new Et(this), this.__v_isRef = !0, this.deps = void 0, this.depsTail = void 0, this.flags = 16, this.globalVersion = Ce - 1, this.next = void 0, this.effect = this, this.__v_isReadonly = !n, this.isSSR = r;
  }
  /**
   * @internal
   */
  notify() {
    if (this.flags |= 16, !(this.flags & 8) && // avoid infinite self recursion
    _ !== this)
      return Yt(this, !0), !0;
    process.env.NODE_ENV;
  }
  get value() {
    const t = process.env.NODE_ENV !== "production" ? this.dep.track({
      target: this,
      type: "get",
      key: "value"
    }) : this.dep.track();
    return Xt(this), t && (t.version = this.dep.version), this._value;
  }
  set value(t) {
    this.setter ? this.setter(t) : process.env.NODE_ENV !== "production" && q("Write operation failed: computed value is readonly");
  }
}
// @__NO_SIDE_EFFECTS__
function gs(e, t, n = !1) {
  let r, s;
  V(e) ? r = e : (r = e.get, s = e.set);
  const o = new hs(r, s, n);
  return process.env.NODE_ENV, o;
}
const ze = {}, Ue = /* @__PURE__ */ new WeakMap();
let oe;
function ms(e, t = !1, n = oe) {
  if (n) {
    let r = Ue.get(n);
    r || Ue.set(n, r = []), r.push(e);
  } else process.env.NODE_ENV !== "production" && !t && q(
    "onWatcherCleanup() was called when there was no active watcher to associate with."
  );
}
function vs(e, t, n = Oe) {
  const { immediate: r, deep: s, once: o, scheduler: i, augmentJob: l, call: a } = n, u = (h) => {
    (n.onWarn || q)(
      "Invalid watch source: ",
      h,
      "A watch source can only be a getter/effect function, a ref, a reactive object, or an array of these types."
    );
  }, d = (h) => s ? h : /* @__PURE__ */ T(h) || s === !1 || s === 0 ? ne(h, 1) : ne(h);
  let c, f, g, x, m = !1, N = !1;
  if (/* @__PURE__ */ P(e) ? (f = () => e.value, m = /* @__PURE__ */ T(e)) : /* @__PURE__ */ le(e) ? (f = () => d(e), m = !0) : E(e) ? (N = !0, m = e.some((h) => /* @__PURE__ */ le(h) || /* @__PURE__ */ T(h)), f = () => e.map((h) => {
    if (/* @__PURE__ */ P(h))
      return h.value;
    if (/* @__PURE__ */ le(h))
      return d(h);
    if (V(h))
      return a ? a(h, 2) : h();
    process.env.NODE_ENV !== "production" && u(h);
  })) : V(e) ? t ? f = a ? () => a(e, 2) : e : f = () => {
    if (g) {
      be();
      try {
        g();
      } finally {
        ye();
      }
    }
    const h = oe;
    oe = c;
    try {
      return a ? a(e, 3, [x]) : e(x);
    } finally {
      oe = h;
    }
  } : (f = he, process.env.NODE_ENV !== "production" && u(e)), t && s) {
    const h = f, R = s === !0 ? 1 / 0 : s;
    f = () => ne(h(), R);
  }
  const D = () => {
    c.stop();
  };
  if (o && t) {
    const h = t;
    t = (...R) => {
      const M = h(...R);
      return D(), M;
    };
  }
  let j = N ? new Array(e.length).fill(ze) : ze;
  const b = (h) => {
    if (!(!(c.flags & 1) || !c.dirty && !h))
      if (t) {
        const R = c.run();
        if (h || s || m || (N ? R.some((M, Y) => B(M, j[Y])) : B(R, j))) {
          g && g();
          const M = oe;
          oe = c;
          try {
            const Y = [
              R,
              // pass undefined as the old value when it's changed for the first time
              j === ze ? void 0 : N && j[0] === ze ? [] : j,
              x
            ];
            j = R, a ? a(t, 3, Y) : (
              // @ts-expect-error
              t(...Y)
            );
          } finally {
            oe = M;
          }
        }
      } else
        c.run();
  };
  return l && l(b), c = new qn(f), c.scheduler = i ? () => i(b, !1) : b, x = (h) => ms(h, !1, c), g = c.onStop = () => {
    const h = Ue.get(c);
    if (h) {
      if (a)
        a(h, 4);
      else
        for (const R of h) R();
      Ue.delete(c);
    }
  }, process.env.NODE_ENV !== "production" && (c.onTrack = n.onTrack, c.onTrigger = n.onTrigger), t ? r ? b(!0) : j = c.run() : i ? i(b.bind(null, !0), !0) : c.run(), D.pause = c.pause.bind(c), D.resume = c.resume.bind(c), D.stop = D, D;
}
function ne(e, t = 1 / 0, n) {
  if (t <= 0 || !A(e) || e.__v_skip || (n = n || /* @__PURE__ */ new Map(), (n.get(e) || 0) >= t))
    return e;
  if (n.set(e, t), t--, /* @__PURE__ */ P(e))
    ne(e.value, t, n);
  else if (E(e))
    for (let r = 0; r < e.length; r++)
      ne(e[r], t, n);
  else if (Kt(e) || ie(e))
    e.forEach((r) => {
      ne(r, t, n);
    });
  else if (Ut(e)) {
    for (const r in e)
      ne(e[r], t, n);
    for (const r of Object.getOwnPropertySymbols(e))
      Object.prototype.propertyIsEnumerable.call(e, r) && ne(e[r], t, n);
  }
  return e;
}
/**
* @vue/runtime-core v3.5.39
* (c) 2018-present Yuxi (Evan) You and Vue contributors
* @license MIT
**/
const ce = [];
function bs(e) {
  ce.push(e);
}
function ys() {
  ce.pop();
}
let ot = !1;
function I(e, ...t) {
  if (ot) return;
  ot = !0, be();
  const n = ce.length ? ce[ce.length - 1].component : null, r = n && n.appContext.config.warnHandler, s = _s();
  if (r)
    Ze(
      r,
      n,
      11,
      [
        // eslint-disable-next-line no-restricted-syntax
        e + t.map((o) => {
          var i, l;
          return (l = (i = o.toString) == null ? void 0 : i.call(o)) != null ? l : JSON.stringify(o);
        }).join(""),
        n && n.proxy,
        s.map(
          ({ vnode: o }) => `at <${On(n, o.type)}>`
        ).join(`
`),
        s
      ]
    );
  else {
    const o = [`[Vue warn]: ${e}`, ...t];
    s.length && o.push(`
`, ...xs(s)), console.warn(...o);
  }
  ye(), ot = !1;
}
function _s() {
  let e = ce[ce.length - 1];
  if (!e)
    return [];
  const t = [];
  for (; e; ) {
    const n = t[0];
    n && n.vnode === e ? n.recurseCount++ : t.push({
      vnode: e,
      recurseCount: 0
    });
    const r = e.component && e.component.parent;
    e = r && r.vnode;
  }
  return t;
}
function xs(e) {
  const t = [];
  return e.forEach((n, r) => {
    t.push(...r === 0 ? [] : [`
`], ...ws(n));
  }), t;
}
function ws({ vnode: e, recurseCount: t }) {
  const n = t > 0 ? `... (${t} recursive calls)` : "", r = e.component ? e.component.parent == null : !1, s = ` at <${On(
    e.component,
    e.type,
    r
  )}`, o = ">" + n;
  return e.props ? [s, ...ks(e.props), o] : [s + o];
}
function ks(e) {
  const t = [], n = Object.keys(e);
  return n.slice(0, 3).forEach((r) => {
    t.push(...ln(r, e[r]));
  }), n.length > 3 && t.push(" ..."), t;
}
function ln(e, t, n) {
  return F(t) ? (t = JSON.stringify(t), n ? t : [`${e}=${t}`]) : typeof t == "number" || typeof t == "boolean" || t == null ? n ? t : [`${e}=${t}`] : /* @__PURE__ */ P(t) ? (t = ln(e, /* @__PURE__ */ v(t.value), !0), n ? t : [`${e}=Ref<`, t, ">"]) : V(t) ? [`${e}=fn${t.name ? `<${t.name}>` : ""}`] : (t = /* @__PURE__ */ v(t), n ? t : [`${e}=`, t]);
}
const St = {
  sp: "serverPrefetch hook",
  bc: "beforeCreate hook",
  c: "created hook",
  bm: "beforeMount hook",
  m: "mounted hook",
  bu: "beforeUpdate hook",
  u: "updated",
  bum: "beforeUnmount hook",
  um: "unmounted hook",
  a: "activated hook",
  da: "deactivated hook",
  ec: "errorCaptured hook",
  rtc: "renderTracked hook",
  rtg: "renderTriggered hook",
  0: "setup function",
  1: "render function",
  2: "watcher getter",
  3: "watcher callback",
  4: "watcher cleanup function",
  5: "native event handler",
  6: "component event handler",
  7: "vnode hook",
  8: "directive hook",
  9: "transition hook",
  10: "app errorHandler",
  11: "app warnHandler",
  12: "ref function",
  13: "async component loader",
  14: "scheduler flush",
  15: "component update",
  16: "app unmount cleanup function"
};
function Ze(e, t, n, r) {
  try {
    return r ? e(...r) : e();
  } catch (s) {
    Ct(s, t, n);
  }
}
function Ot(e, t, n, r) {
  if (V(e)) {
    const s = Ze(e, t, n, r);
    return s && jn(s) && s.catch((o) => {
      Ct(o, t, n);
    }), s;
  }
  if (E(e)) {
    const s = [];
    for (let o = 0; o < e.length; o++)
      s.push(Ot(e[o], t, n, r));
    return s;
  } else process.env.NODE_ENV !== "production" && I(
    `Invalid value type passed to callWithAsyncErrorHandling(): ${typeof e}`
  );
}
function Ct(e, t, n, r = !0) {
  const s = t ? t.vnode : null, { errorHandler: o, throwUnhandledErrorInProduction: i } = t && t.appContext.config || Oe;
  if (t) {
    let l = t.parent;
    const a = t.proxy, u = process.env.NODE_ENV !== "production" ? St[n] : `https://vuejs.org/error-reference/#runtime-${n}`;
    for (; l; ) {
      const d = l.ec;
      if (d) {
        for (let c = 0; c < d.length; c++)
          if (d[c](e, a, u) === !1)
            return;
      }
      l = l.parent;
    }
    if (o) {
      be(), Ze(o, null, 10, [
        e,
        a,
        u
      ]), ye();
      return;
    }
  }
  Es(e, n, s, r, i);
}
function Es(e, t, n, r = !0, s = !1) {
  if (process.env.NODE_ENV !== "production") {
    const o = St[t];
    if (n && bs(n), I(`Unhandled error${o ? ` during execution of ${o}` : ""}`), n && ys(), r)
      throw e;
    console.error(e);
  } else {
    if (s)
      throw e;
    console.error(e);
  }
}
const z = [];
let Q = -1;
const ge = [];
let ee = null, fe = 0;
const cn = /* @__PURE__ */ Promise.resolve();
let Le = null;
const Ns = 100;
function un(e) {
  const t = Le || cn;
  return e ? t.then(this ? e.bind(this) : e) : t;
}
function Ss(e) {
  let t = Q + 1, n = z.length;
  for (; t < n; ) {
    const r = t + n >>> 1, s = z[r], o = Ie(s);
    o < e || o === e && s.flags & 2 ? t = r + 1 : n = r;
  }
  return t;
}
function dn(e) {
  if (!(e.flags & 1)) {
    const t = Ie(e), n = z[z.length - 1];
    !n || // fast path when the job id is larger than the tail
    !(e.flags & 2) && t >= Ie(n) ? z.push(e) : z.splice(Ss(t), 0, e), e.flags |= 1, fn();
  }
}
function fn() {
  Le || (Le = cn.then(hn));
}
function pn(e) {
  E(e) ? ge.push(...e) : ee && e.id === -1 ? ee.splice(fe + 1, 0, e) : e.flags & 1 || (ge.push(e), e.flags |= 1), fn();
}
function Os(e) {
  if (ge.length) {
    const t = [...new Set(ge)].sort(
      (n, r) => Ie(n) - Ie(r)
    );
    if (ge.length = 0, ee) {
      ee.push(...t);
      return;
    }
    for (ee = t, process.env.NODE_ENV !== "production" && (e = e || /* @__PURE__ */ new Map()), fe = 0; fe < ee.length; fe++) {
      const n = ee[fe];
      process.env.NODE_ENV !== "production" && gn(e, n) || (n.flags & 4 && (n.flags &= -2), n.flags & 8 || n(), n.flags &= -2);
    }
    ee = null, fe = 0;
  }
}
const Ie = (e) => e.id == null ? e.flags & 2 ? -1 : 1 / 0 : e.id;
function hn(e) {
  process.env.NODE_ENV !== "production" && (e = e || /* @__PURE__ */ new Map());
  const t = process.env.NODE_ENV !== "production" ? (n) => gn(e, n) : he;
  try {
    for (Q = 0; Q < z.length; Q++) {
      const n = z[Q];
      if (n && !(n.flags & 8)) {
        if (process.env.NODE_ENV !== "production" && t(n))
          continue;
        n.flags & 4 && (n.flags &= -2), Ze(
          n,
          n.i,
          n.i ? 15 : 14
        ), n.flags & 4 || (n.flags &= -2);
      }
    }
  } finally {
    for (; Q < z.length; Q++) {
      const n = z[Q];
      n && (n.flags &= -2);
    }
    Q = -1, z.length = 0, Os(e), Le = null, (z.length || ge.length) && hn(e);
  }
}
function gn(e, t) {
  const n = e.get(t) || 0;
  if (n > Ns) {
    const r = t.i, s = r && Sn(r.type);
    return Ct(
      `Maximum recursive updates exceeded${s ? ` in component <${s}>` : ""}. This means you have a reactive effect that is mutating its own dependencies and thus recursively triggering itself. Possible sources include component template, render function, updated hook or watcher source function.`,
      null,
      10
    ), !0;
  }
  return e.set(t, n + 1), !1;
}
const it = /* @__PURE__ */ new Map();
process.env.NODE_ENV !== "production" && (Qe().__VUE_HMR_RUNTIME__ = {
  createRecord: at(Cs),
  rerender: at(Ds),
  reload: at(Is)
});
const We = /* @__PURE__ */ new Map();
function Cs(e, t) {
  return We.has(e) ? !1 : (We.set(e, {
    initialDef: Be(t),
    instances: /* @__PURE__ */ new Set()
  }), !0);
}
function Be(e) {
  return Cn(e) ? e.__vccOpts : e;
}
function Ds(e, t) {
  const n = We.get(e);
  n && (n.initialDef.render = t, [...n.instances].forEach((r) => {
    t && (r.render = t, Be(r.type).render = t), r.renderCache = [], r.job.flags & 8 || r.update();
  }));
}
function Is(e, t) {
  const n = We.get(e);
  if (!n) return;
  t = Be(t), $t(n.initialDef, t);
  const r = [...n.instances];
  for (let s = 0; s < r.length; s++) {
    const o = r[s], i = Be(o.type);
    let l = it.get(i);
    l || (i !== n.initialDef && $t(i, t), it.set(i, l = /* @__PURE__ */ new Set())), l.add(o), o.appContext.propsCache.delete(o.type), o.appContext.emitsCache.delete(o.type), o.appContext.optionsCache.delete(o.type), o.ceReload ? (l.add(o), o.ceReload(t.styles), l.delete(o)) : o.parent ? dn(() => {
      o.job.flags & 8 || (o.parent.update(), l.delete(o));
    }) : o.appContext.reload ? o.appContext.reload() : typeof window < "u" ? window.location.reload() : console.warn(
      "[HMR] Root or manually mounted instance modified. Full reload required."
    ), o.root.ce && o !== o.root && o.root.ce._removeChildStyle(i);
  }
  pn(() => {
    it.clear();
  });
}
function $t(e, t) {
  J(e, t);
  for (const n in e)
    n !== "__file" && !(n in t) && delete e[n];
}
function at(e) {
  return (t, n) => {
    try {
      return e(t, n);
    } catch (r) {
      console.error(r), console.warn(
        "[HMR] Something went wrong during Vue component hot-reload. Full reload required."
      );
    }
  };
}
let pe, je = [];
function mn(e, t) {
  var n, r;
  pe = e, pe ? (pe.enabled = !0, je.forEach(({ event: s, args: o }) => pe.emit(s, ...o)), je = []) : /* handle late devtools injection - only do this if we are in an actual */ /* browser environment to avoid the timer handle stalling test runner exit */ /* (#4815) */ typeof window < "u" && // some envs mock window but not fully
  window.HTMLElement && // also exclude jsdom
  // eslint-disable-next-line no-restricted-syntax
  !((r = (n = window.navigator) == null ? void 0 : n.userAgent) != null && r.includes("jsdom")) ? ((t.__VUE_DEVTOOLS_HOOK_REPLAY__ = t.__VUE_DEVTOOLS_HOOK_REPLAY__ || []).push((o) => {
    mn(o, t);
  }), setTimeout(() => {
    pe || (t.__VUE_DEVTOOLS_HOOK_REPLAY__ = null, je = []);
  }, 3e3)) : je = [];
}
let Ve = null, Vs = null;
function ke(e, t) {
  return process.env.NODE_ENV !== "production" && I("withDirectives can only be used inside render functions."), e;
}
function vn(e, t, n = !1) {
  const r = Nn();
  if (r || Fs) {
    let s = r ? r.parent == null || r.ce ? r.vnode.appContext && r.vnode.appContext.provides : r.parent.provides : void 0;
    if (s && e in s)
      return s[e];
    if (arguments.length > 1)
      return n && V(t) ? t.call(r && r.proxy) : t;
    process.env.NODE_ENV !== "production" && I(`injection "${String(e)}" not found.`);
  } else process.env.NODE_ENV !== "production" && I("inject() can only be used inside setup() or functional components.");
}
const As = /* @__PURE__ */ Symbol.for("v-scx"), Rs = () => {
  {
    const e = vn(As);
    return e || process.env.NODE_ENV !== "production" && I(
      "Server rendering context not provided. Make sure to only call useSSRContext() conditionally in the server build."
    ), e;
  }
};
function Ts(e, t, n) {
  return process.env.NODE_ENV !== "production" && !V(t) && I(
    "`watch(fn, options?)` signature has been moved to a separate API. Use `watchEffect(fn, options?)` instead. `watch` now only supports `watch(source, cb, options?) signature."
  ), $s(e, t, n);
}
function $s(e, t, n = Oe) {
  const { immediate: r, deep: s, flush: o, once: i } = n;
  process.env.NODE_ENV !== "production" && !t && (r !== void 0 && I(
    'watch() "immediate" option is only respected when using the watch(source, callback, options?) signature.'
  ), s !== void 0 && I(
    'watch() "deep" option is only respected when using the watch(source, callback, options?) signature.'
  ), i !== void 0 && I(
    'watch() "once" option is only respected when using the watch(source, callback, options?) signature.'
  ));
  const l = J({}, n);
  process.env.NODE_ENV !== "production" && (l.onWarn = I);
  const a = t && r || !t && o !== "post";
  let u;
  if (Re) {
    if (o === "sync") {
      const g = Rs();
      u = g.__watcherHandles || (g.__watcherHandles = []);
    } else if (!a) {
      const g = () => {
      };
      return g.stop = he, g.resume = he, g.pause = he, g;
    }
  }
  const d = _e;
  l.call = (g, x, m) => Ot(g, d, x, m);
  let c = !1;
  o === "post" ? l.scheduler = (g) => {
    Ls(g, d && d.suspense);
  } : o !== "sync" && (c = !0, l.scheduler = (g, x) => {
    x ? g() : dn(g);
  }), l.augmentJob = (g) => {
    t && (g.flags |= 4), c && (g.flags |= 2, d && (g.id = d.uid, g.i = d));
  };
  const f = vs(e, t, l);
  return Re && (u ? u.push(f) : a && f()), f;
}
const Ps = (e) => e.__isTeleport;
function bn(e, t) {
  e.shapeFlag & 6 && e.component ? (e.transition = t, bn(e.component.subTree, t)) : e.shapeFlag & 128 ? (e.ssContent.transition = t.clone(e.ssContent), e.ssFallback.transition = t.clone(e.ssFallback)) : e.transition = t;
}
// @__NO_SIDE_EFFECTS__
function $e(e, t) {
  return V(e) ? (
    // #8236: extend call and options.name access are considered side-effects
    // by Rollup, so we have to wrap it in a pure-annotated IIFE.
    J({ name: e.name }, t, { setup: e })
  ) : e;
}
Qe().requestIdleCallback;
Qe().cancelIdleCallback;
function Ms(e, t, n = _e, r = !1) {
  if (n) {
    const s = n[e] || (n[e] = []), o = t.__weh || (t.__weh = (...i) => {
      be();
      const l = Zs(n), a = Ot(t, n, e, i);
      return l(), ye(), a;
    });
    return r ? s.unshift(o) : s.push(o), o;
  } else if (process.env.NODE_ENV !== "production") {
    const s = Fn(St[e].replace(/ hook$/, ""));
    I(
      `${s} is called when there is no active component instance to be associated with. Lifecycle injection APIs can only be used during execution of setup(). If you are using async setup(), make sure to register lifecycle hooks before the first await statement.`
    );
  }
}
const zs = (e) => (t, n = _e) => {
  (!Re || e === "sp") && Ms(e, (...r) => t(...r), n);
}, js = zs("m"), Ks = /* @__PURE__ */ Symbol.for("v-ndc");
function Je(e, t, n, r) {
  let s;
  const o = n, i = E(e);
  if (i || F(e)) {
    const l = i && /* @__PURE__ */ le(e);
    let a = !1, u = !1;
    l && (a = !/* @__PURE__ */ T(e), u = /* @__PURE__ */ U(e), e = Xe(e)), s = new Array(e.length);
    for (let d = 0, c = e.length; d < c; d++)
      s[d] = t(
        a ? u ? ve(L(e[d])) : L(e[d]) : e[d],
        d,
        void 0,
        o
      );
  } else if (typeof e == "number")
    if (process.env.NODE_ENV !== "production" && (!Number.isInteger(e) || e < 0))
      I(
        `The v-for range expects a positive integer value but got ${e}.`
      ), s = [];
    else {
      s = new Array(e);
      for (let l = 0; l < e; l++)
        s[l] = t(l + 1, l, void 0, o);
    }
  else if (A(e))
    if (e[Symbol.iterator])
      s = Array.from(
        e,
        (l, a) => t(l, a, void 0, o)
      );
    else {
      const l = Object.keys(e);
      s = new Array(l.length);
      for (let a = 0, u = l.length; a < u; a++) {
        const d = l[a];
        s[a] = t(e[d], d, a, o);
      }
    }
  else
    s = [];
  return s;
}
const Hs = {};
process.env.NODE_ENV !== "production" && (Hs.ownKeys = (e) => (I(
  "Avoid app logic that relies on enumerating keys on a component instance. The keys will be empty in production mode to avoid performance overhead."
), Reflect.ownKeys(e)));
let Fs = null;
const Us = {}, yn = (e) => Object.getPrototypeOf(e) === Us, Ls = Bs, Ws = (e) => e.__isSuspense;
function Bs(e, t) {
  t && t.pendingBranch ? E(e) ? t.effects.push(...e) : t.effects.push(e) : pn(e);
}
const se = /* @__PURE__ */ Symbol.for("v-fgt"), Js = /* @__PURE__ */ Symbol.for("v-txt"), gt = /* @__PURE__ */ Symbol.for("v-cmt"), Ke = [];
let K = null;
function y(e = !1) {
  Ke.push(K = e ? null : []);
}
function qs() {
  Ke.pop(), K = Ke[Ke.length - 1] || null;
}
function _n(e) {
  return e.dynamicChildren = K || $n, qs(), K && K.push(e), e;
}
function w(e, t, n, r, s, o) {
  return _n(
    p(
      e,
      t,
      n,
      r,
      s,
      o,
      !0
    )
  );
}
function mt(e, t, n, r, s) {
  return _n(
    Ae(
      e,
      t,
      n,
      r,
      s,
      !0
    )
  );
}
function Ys(e) {
  return e ? e.__v_isVNode === !0 : !1;
}
const Gs = (...e) => wn(
  ...e
), xn = ({ key: e }) => e ?? null, He = ({
  ref: e,
  ref_key: t,
  ref_for: n
}) => (typeof e == "number" && (e = "" + e), e != null ? F(e) || /* @__PURE__ */ P(e) || V(e) ? { i: Ve, r: e, k: t, f: !!n } : e : null);
function p(e, t = null, n = null, r = 0, s = null, o = e === se ? 0 : 1, i = !1, l = !1) {
  const a = {
    __v_isVNode: !0,
    __v_skip: !0,
    type: e,
    props: t,
    key: t && xn(t),
    ref: t && He(t),
    scopeId: Vs,
    slotScopeIds: null,
    children: n,
    component: null,
    suspense: null,
    ssContent: null,
    ssFallback: null,
    dirs: null,
    transition: null,
    el: null,
    anchor: null,
    target: null,
    targetStart: null,
    targetAnchor: null,
    staticCount: 0,
    shapeFlag: o,
    patchFlag: r,
    dynamicProps: s,
    dynamicChildren: null,
    appContext: null,
    ctx: Ve
  };
  return l ? (Ye(a, n), o & 128 && e.normalize(a)) : n && (a.shapeFlag |= F(n) ? 8 : 16), process.env.NODE_ENV !== "production" && a.key !== a.key && I("VNode created with invalid key (NaN). VNode type:", a.type), // avoid a block node from tracking itself
  !i && // has current parent block
  K && // presence of a patch flag indicates this node needs patching on updates.
  // component nodes also should always be patched, because even if the
  // component doesn't need to update, it needs to persist the instance on to
  // the next vnode so that it can be properly unmounted later.
  (a.patchFlag > 0 || o & 6) && // the EVENTS flag is only for hydration and if it is the only flag, the
  // vnode should not be considered dynamic due to handler caching.
  a.patchFlag !== 32 && K.push(a), a;
}
const Ae = process.env.NODE_ENV !== "production" ? Gs : wn;
function wn(e, t = null, n = null, r = 0, s = null, o = !1) {
  if ((!e || e === Ks) && (process.env.NODE_ENV !== "production" && !e && I(`Invalid vnode type when creating vnode: ${e}.`), e = gt), Ys(e)) {
    const l = qe(
      e,
      t,
      !0
      /* mergeRef: true */
    );
    return n && Ye(l, n), !o && K && (l.shapeFlag & 6 ? K[K.indexOf(e)] = l : K.push(l)), l.patchFlag = -2, l;
  }
  if (Cn(e) && (e = e.__vccOpts), t) {
    t = Qs(t);
    let { class: l, style: a } = t;
    l && !F(l) && (t.class = me(l)), A(a) && (/* @__PURE__ */ Fe(a) && !E(a) && (a = J({}, a)), t.style = _t(a));
  }
  const i = F(e) ? 1 : Ws(e) ? 128 : Ps(e) ? 64 : A(e) ? 4 : V(e) ? 2 : 0;
  return process.env.NODE_ENV !== "production" && i & 4 && /* @__PURE__ */ Fe(e) && (e = /* @__PURE__ */ v(e), I(
    "Vue received a Component that was made a reactive object. This can lead to unnecessary performance overhead and should be avoided by marking the component with `markRaw` or using `shallowRef` instead of `ref`.",
    `
Component that was made reactive: `,
    e
  )), p(
    e,
    t,
    n,
    r,
    s,
    i,
    o,
    !0
  );
}
function Qs(e) {
  return e ? /* @__PURE__ */ Fe(e) || yn(e) ? J({}, e) : e : null;
}
function qe(e, t, n = !1, r = !1) {
  const { props: s, ref: o, patchFlag: i, children: l, transition: a } = e, u = t ? Xs(s || {}, t) : s, d = {
    __v_isVNode: !0,
    __v_skip: !0,
    type: e.type,
    props: u,
    key: u && xn(u),
    ref: t && t.ref ? (
      // #2078 in the case of <component :is="vnode" ref="extra"/>
      // if the vnode itself already has a ref, cloneVNode will need to merge
      // the refs so the single vnode can be set on multiple refs
      n && o ? E(o) ? o.concat(He(t)) : [o, He(t)] : He(t)
    ) : o,
    scopeId: e.scopeId,
    slotScopeIds: e.slotScopeIds,
    children: process.env.NODE_ENV !== "production" && i === -1 && E(l) ? l.map(kn) : l,
    target: e.target,
    targetStart: e.targetStart,
    targetAnchor: e.targetAnchor,
    staticCount: e.staticCount,
    shapeFlag: e.shapeFlag,
    // if the vnode is cloned with extra props, we can no longer assume its
    // existing patch flag to be reliable and need to add the FULL_PROPS flag.
    // note: preserve flag for fragments since they use the flag for children
    // fast paths only.
    patchFlag: t && e.type !== se ? i === -1 ? 16 : i | 16 : i,
    dynamicProps: e.dynamicProps,
    dynamicChildren: e.dynamicChildren,
    appContext: e.appContext,
    dirs: e.dirs,
    transition: a,
    // These should technically only be non-null on mounted VNodes. However,
    // they *should* be copied for kept-alive vnodes. So we just always copy
    // them since them being non-null during a mount doesn't affect the logic as
    // they will simply be overwritten.
    component: e.component,
    suspense: e.suspense,
    ssContent: e.ssContent && qe(e.ssContent),
    ssFallback: e.ssFallback && qe(e.ssFallback),
    placeholder: e.placeholder,
    el: e.el,
    anchor: e.anchor,
    ctx: e.ctx,
    ce: e.ce
  };
  return a && r && bn(
    d,
    a.clone(d)
  ), d;
}
function kn(e) {
  const t = qe(e);
  return E(e.children) && (t.children = e.children.map(kn)), t;
}
function En(e = " ", t = 0) {
  return Ae(Js, null, e, t);
}
function re(e = "", t = !1) {
  return t ? (y(), mt(gt, null, e)) : Ae(gt, null, e);
}
function Ye(e, t) {
  let n = 0;
  const { shapeFlag: r } = e;
  if (t == null)
    t = null;
  else if (E(t))
    n = 16;
  else if (typeof t == "object")
    if (r & 65) {
      const s = t.default;
      s && (s._c && (s._d = !1), Ye(e, s()), s._c && (s._d = !0));
      return;
    } else
      n = 32, !t._ && !yn(t) && (t._ctx = Ve);
  else if (V(t)) {
    if (r & 65) {
      Ye(e, { default: t });
      return;
    }
    t = { default: t, _ctx: Ve }, n = 32;
  } else
    t = String(t), r & 64 ? (n = 16, t = [En(t)]) : n = 8;
  e.children = t, e.shapeFlag |= n;
}
function Xs(...e) {
  const t = {};
  for (let n = 0; n < e.length; n++) {
    const r = e[n];
    for (const s in r)
      if (s === "class")
        t.class !== r.class && (t.class = me([t.class, r.class]));
      else if (s === "style")
        t.style = _t([t.style, r.style]);
      else if (Pn(s)) {
        const o = t[s], i = r[s];
        i && o !== i && !(E(o) && o.includes(i)) ? t[s] = o ? [].concat(o, i) : i : i == null && o == null && // mergeProps({ 'onUpdate:modelValue': undefined }) should not retain
        // the model listener.
        !Mn(s) && (t[s] = i);
      } else s !== "" && (t[s] = r[s]);
  }
  return t;
}
let _e = null;
const Nn = () => _e || Ve;
let vt;
{
  const e = Qe(), t = (n, r) => {
    let s;
    return (s = e[n]) || (s = e[n] = []), s.push(r), (o) => {
      s.length > 1 ? s.forEach((i) => i(o)) : s[0](o);
    };
  };
  vt = t(
    "__VUE_INSTANCE_SETTERS__",
    (n) => _e = n
  ), t(
    "__VUE_SSR_SETTERS__",
    (n) => Re = n
  );
}
const Zs = (e) => {
  const t = _e;
  return vt(e), e.scope.on(), () => {
    e.scope.off(), vt(t);
  };
};
let Re = !1;
process.env.NODE_ENV;
const er = /(?:^|[-_])\w/g, tr = (e) => e.replace(er, (t) => t.toUpperCase()).replace(/[-_]/g, "");
function Sn(e, t = !0) {
  return V(e) ? e.displayName || e.name : e.name || t && e.__name;
}
function On(e, t, n = !1) {
  let r = Sn(t);
  if (!r && t.__file) {
    const s = t.__file.match(/([^/\\]+)\.\w+$/);
    s && (r = s[1]);
  }
  if (!r && e) {
    const s = (o) => {
      for (const i in o)
        if (o[i] === t)
          return i;
    };
    r = s(e.components) || e.parent && s(
      e.parent.type.components
    ) || s(e.appContext.components);
  }
  return r ? tr(r) : n ? "App" : "Anonymous";
}
function Cn(e) {
  return V(e) && "__vccOpts" in e;
}
const Te = (e, t) => {
  const n = /* @__PURE__ */ gs(e, t, Re);
  if (process.env.NODE_ENV !== "production") {
    const r = Nn();
    r && r.appContext.config.warnRecursiveComputed && (n._warnRecursive = !0);
  }
  return n;
};
function nr() {
  if (process.env.NODE_ENV === "production" || typeof window > "u")
    return;
  const e = { style: "color:#3ba776" }, t = { style: "color:#1677ff" }, n = { style: "color:#f5222d" }, r = { style: "color:#eb2f96" }, s = {
    __vue_custom_formatter: !0,
    header(c) {
      if (!A(c))
        return null;
      if (c.__isVue)
        return ["div", e, "VueInstance"];
      if (/* @__PURE__ */ P(c)) {
        be();
        const f = c.value;
        return ye(), [
          "div",
          {},
          ["span", e, d(c)],
          "<",
          l(f),
          ">"
        ];
      } else {
        if (/* @__PURE__ */ le(c))
          return [
            "div",
            {},
            ["span", e, /* @__PURE__ */ T(c) ? "ShallowReactive" : "Reactive"],
            "<",
            l(c),
            `>${/* @__PURE__ */ U(c) ? " (readonly)" : ""}`
          ];
        if (/* @__PURE__ */ U(c))
          return [
            "div",
            {},
            ["span", e, /* @__PURE__ */ T(c) ? "ShallowReadonly" : "Readonly"],
            "<",
            l(c),
            ">"
          ];
      }
      return null;
    },
    hasBody(c) {
      return c && c.__isVue;
    },
    body(c) {
      if (c && c.__isVue)
        return [
          "div",
          {},
          ...o(c.$)
        ];
    }
  };
  function o(c) {
    const f = [];
    c.type.props && c.props && f.push(i("props", /* @__PURE__ */ v(c.props))), c.setupState !== Oe && f.push(i("setup", c.setupState)), c.data !== Oe && f.push(i("data", /* @__PURE__ */ v(c.data)));
    const g = a(c, "computed");
    g && f.push(i("computed", g));
    const x = a(c, "inject");
    return x && f.push(i("injected", x)), f.push([
      "div",
      {},
      [
        "span",
        {
          style: r.style + ";opacity:0.66"
        },
        "$ (internal): "
      ],
      ["object", { object: c }]
    ]), f;
  }
  function i(c, f) {
    return f = J({}, f), Object.keys(f).length ? [
      "div",
      { style: "line-height:1.25em;margin-bottom:0.6em" },
      [
        "div",
        {
          style: "color:#476582"
        },
        c
      ],
      [
        "div",
        {
          style: "padding-left:1.25em"
        },
        ...Object.keys(f).map((g) => [
          "div",
          {},
          ["span", r, g + ": "],
          l(f[g], !1)
        ])
      ]
    ] : ["span", {}];
  }
  function l(c, f = !0) {
    return typeof c == "number" ? ["span", t, c] : typeof c == "string" ? ["span", n, JSON.stringify(c)] : typeof c == "boolean" ? ["span", r, c] : A(c) ? ["object", { object: f ? /* @__PURE__ */ v(c) : c }] : ["span", n, String(c)];
  }
  function a(c, f) {
    const g = c.type;
    if (V(g))
      return;
    const x = {};
    for (const m in c.ctx)
      u(g, m, f) && (x[m] = c.ctx[m]);
    return x;
  }
  function u(c, f, g) {
    const x = c[g];
    if (E(x) && x.includes(f) || A(x) && f in x || c.extends && u(c.extends, f, g) || c.mixins && c.mixins.some((m) => u(m, f, g)))
      return !0;
  }
  function d(c) {
    return /* @__PURE__ */ T(c) ? "ShallowRef" : c.effect ? "ComputedRef" : "Ref";
  }
  window.devtoolsFormatters ? window.devtoolsFormatters.push(s) : window.devtoolsFormatters = [s];
}
const sr = process.env.NODE_ENV !== "production" ? I : he;
process.env.NODE_ENV;
process.env.NODE_ENV;
/**
* @vue/runtime-dom v3.5.39
* (c) 2018-present Yuxi (Evan) You and Vue contributors
* @license MIT
**/
let rr;
const Pt = typeof window < "u" && window.trustedTypes;
if (Pt)
  try {
    rr = /* @__PURE__ */ Pt.createPolicy("vue", {
      createHTML: (e) => e
    });
  } catch (e) {
    process.env.NODE_ENV !== "production" && sr(`Error creating trusted types policy: ${e}`);
  }
process.env.NODE_ENV;
function we(e, t, n, r) {
  e.addEventListener(t, n, r);
}
const Mt = (e) => {
  const t = e.props["onUpdate:modelValue"] || !1;
  return E(t) ? (n) => Un(t, n) : t;
};
function or(e) {
  e.target.composing = !0;
}
function zt(e) {
  const t = e.target;
  t.composing && (t.composing = !1, t.dispatchEvent(new Event("input")));
}
const lt = /* @__PURE__ */ Symbol("_assign");
function jt(e, t, n) {
  return t && (e = e.trim()), n && (e = Wt(e)), e;
}
const Ee = {
  created(e, { modifiers: { lazy: t, trim: n, number: r } }, s) {
    e[lt] = Mt(s);
    const o = r || s.props && s.props.type === "number";
    we(e, t ? "change" : "input", (i) => {
      i.target.composing || e[lt](jt(e.value, n, o));
    }), (n || o) && we(e, "change", () => {
      e.value = jt(e.value, n, o);
    }), t || (we(e, "compositionstart", or), we(e, "compositionend", zt), we(e, "change", zt));
  },
  // set value on mounted so it's after min/max for type="range"
  mounted(e, { value: t }) {
    e.value = t ?? "";
  },
  beforeUpdate(e, { value: t, oldValue: n, modifiers: { lazy: r, trim: s, number: o } }, i) {
    if (e[lt] = Mt(i), e.composing) return;
    const l = (o || e.type === "number") && !/^0\d/.test(e.value) ? Wt(e.value) : e.value, a = t ?? "";
    if (l === a)
      return;
    const u = e.getRootNode();
    (u instanceof Document || u instanceof ShadowRoot) && u.activeElement === e && e.type !== "range" && (r && t === n || s && e.value.trim() === a) || (e.value = a);
  }
}, ir = ["ctrl", "shift", "alt", "meta"], ar = {
  stop: (e) => e.stopPropagation(),
  prevent: (e) => e.preventDefault(),
  self: (e) => e.target !== e.currentTarget,
  ctrl: (e) => !e.ctrlKey,
  shift: (e) => !e.shiftKey,
  alt: (e) => !e.altKey,
  meta: (e) => !e.metaKey,
  left: (e) => "button" in e && e.button !== 0,
  middle: (e) => "button" in e && e.button !== 1,
  right: (e) => "button" in e && e.button !== 2,
  exact: (e, t) => ir.some((n) => e[`${n}Key`] && !t.includes(n))
}, Dn = (e, t) => {
  if (!e) return e;
  const n = e._withMods || (e._withMods = {}), r = t.join(".");
  return n[r] || (n[r] = (s, ...o) => {
    for (let i = 0; i < t.length; i++) {
      const l = ar[t[i]];
      if (l && l(s, t)) return;
    }
    return e(s, ...o);
  });
}, lr = {
  esc: "escape",
  space: " ",
  up: "arrow-up",
  left: "arrow-left",
  right: "arrow-right",
  down: "arrow-down",
  delete: "backspace"
}, cr = (e, t) => {
  const n = e._withKeys || (e._withKeys = {}), r = t.join(".");
  return n[r] || (n[r] = (s) => {
    if (!("key" in s))
      return;
    const o = Hn(s.key);
    if (t.some(
      (i) => i === o || lr[i] === o
    ))
      return e(s);
  });
};
/**
* vue v3.5.39
* (c) 2018-present Yuxi (Evan) You and Vue contributors
* @license MIT
**/
function ur() {
  nr();
}
process.env.NODE_ENV !== "production" && ur();
const dr = {
  key: 0,
  class: "w-7 h-7 rounded-full bg-primary-100 dark:bg-primary-900 flex items-center justify-center text-xs flex-shrink-0 mt-1"
}, fr = {
  key: 0,
  class: "flex items-center gap-1"
}, pr = ["innerHTML"], hr = {
  key: 2,
  class: "whitespace-pre-wrap"
}, gr = {
  key: 1,
  class: "w-7 h-7 rounded-full bg-slate-200 dark:bg-dark-600 flex items-center justify-center text-xs flex-shrink-0 mt-1"
}, mr = /* @__PURE__ */ $e({
  __name: "ChatMessage",
  props: {
    message: {},
    streaming: { type: Boolean }
  },
  setup(e) {
    const t = e, n = Te(() => {
      let r = t.message.content;
      return r = r.replace(/```(\w*)\n([\s\S]*?)```/g, '<pre class="bg-slate-800 text-green-300 rounded p-2 my-1 overflow-x-auto text-xs"><code>$2</code></pre>'), r = r.replace(/`([^`]+)`/g, '<code class="bg-slate-200 dark:bg-dark-600 px-1 rounded text-xs">$1</code>'), r = r.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>"), r = r.replace(/\*([^*]+)\*/g, "<em>$1</em>"), r = r.replace(/\n/g, "<br>"), r;
    });
    return (r, s) => (y(), w("div", {
      class: me(["flex gap-3", e.message.role === "user" ? "justify-end" : "justify-start"])
    }, [
      e.message.role === "assistant" ? (y(), w("div", dr, " AI ")) : re("", !0),
      p("div", {
        class: me([
          "max-w-[85%] rounded-lg px-3 py-2 text-sm leading-relaxed",
          e.message.role === "user" ? "bg-primary-600 text-white" : "bg-slate-100 dark:bg-dark-700 text-slate-800 dark:text-dark-200"
        ])
      }, [
        e.message.role === "assistant" && !e.message.content && e.streaming ? (y(), w("div", fr, [...s[0] || (s[0] = [
          p("span", { class: "inline-block w-1.5 h-4 bg-primary-500 animate-pulse" }, null, -1)
        ])])) : e.message.role === "assistant" ? (y(), w("div", {
          key: 1,
          innerHTML: n.value
        }, null, 8, pr)) : (y(), w("div", hr, X(e.message.content), 1))
      ], 2),
      e.message.role === "user" ? (y(), w("div", gr, " 我 ")) : re("", !0)
    ], 2));
  }
}), vr = { class: "flex gap-2 items-end" }, br = ["placeholder", "disabled", "onKeydown"], yr = ["disabled"], _r = /* @__PURE__ */ $e({
  __name: "ChatInput",
  props: {
    disabled: { type: Boolean, default: !1 },
    placeholder: { default: "输入消息..." }
  },
  emits: ["send"],
  setup(e, { emit: t }) {
    const n = e, r = t, s = /* @__PURE__ */ C(""), o = /* @__PURE__ */ C(null);
    function i() {
      const a = s.value.trim();
      !a || n.disabled || (r("send", a), s.value = "", un(() => l()));
    }
    function l() {
      const a = o.value;
      a && (a.style.height = "auto", a.style.height = Math.min(a.scrollHeight, 120) + "px");
    }
    return (a, u) => (y(), w("div", vr, [
      ke(p("textarea", {
        ref_key: "inputRef",
        ref: o,
        "onUpdate:modelValue": u[0] || (u[0] = (d) => s.value = d),
        placeholder: e.placeholder,
        disabled: e.disabled,
        rows: "1",
        class: "flex-1 resize-none bg-slate-50 dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded-lg px-3 py-2 text-sm text-slate-900 dark:text-white placeholder-slate-400 dark:placeholder-dark-500 focus:border-primary-500 outline-none",
        onKeydown: cr(Dn(i, ["exact", "prevent"]), ["enter"]),
        onInput: l
      }, null, 40, br), [
        [Ee, s.value]
      ]),
      p("button", {
        disabled: e.disabled || !s.value.trim(),
        class: "px-3 py-2 bg-primary-600 hover:bg-primary-700 disabled:opacity-50 disabled:cursor-not-allowed text-white rounded-lg text-sm font-medium transition-colors flex-shrink-0",
        onClick: i
      }, " 发送 ", 8, yr)
    ]));
  }
}), In = [
  { name: "DeepSeek", baseUrl: "https://api.deepseek.com/v1", model: "deepseek-chat" },
  { name: "OpenAI", baseUrl: "https://api.openai.com/v1", model: "gpt-4o-mini" },
  { name: "通义千问", baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1", model: "qwen-turbo" },
  { name: "Moonshot", baseUrl: "https://api.moonshot.cn/v1", model: "moonshot-v1-8k" },
  { name: "智谱", baseUrl: "https://open.bigmodel.cn/api/paas/v4", model: "glm-4-flash" },
  { name: "硅基流动", baseUrl: "https://api.siliconflow.cn/v1", model: "Qwen/Qwen2.5-7B-Instruct" }
], xr = { class: "p-4 space-y-3 border-b border-slate-200 dark:border-dark-700 bg-slate-50 dark:bg-dark-800" }, wr = ["onClick"], kr = { class: "text-sm font-medium text-slate-800 dark:text-white" }, Er = { class: "text-xs text-slate-400 dark:text-dark-500" }, Nr = { class: "flex items-center gap-2" }, Sr = {
  key: 0,
  class: "text-xs text-primary-600 dark:text-primary-400"
}, Or = ["onClick"], Cr = { class: "border border-dashed border-slate-300 dark:border-dark-600 rounded-lg p-3 space-y-2" }, Dr = { class: "flex flex-wrap gap-2" }, Ir = ["onClick"], Vr = {
  key: 0,
  class: "space-y-2 pt-2"
}, Ar = { class: "flex gap-2" }, Rr = ["disabled"], Tr = /* @__PURE__ */ $e({
  __name: "ProviderManager",
  props: {
    providers: {},
    activeProviderName: {}
  },
  emits: ["setActive", "remove", "add"],
  setup(e, { emit: t }) {
    const n = t, r = In, s = /* @__PURE__ */ C(null), o = /* @__PURE__ */ Nt({ name: "", apiKey: "", baseUrl: "", model: "" });
    function i(a) {
      s.value = a, o.name = a.name, o.apiKey = "", o.baseUrl = a.baseUrl, o.model = a.model;
    }
    function l() {
      !o.name || !o.apiKey || (n("add", { name: o.name, apiKey: o.apiKey, baseUrl: o.baseUrl, model: o.model }), s.value = null, o.name = "", o.apiKey = "", o.baseUrl = "", o.model = "");
    }
    return (a, u) => (y(), w("div", xr, [
      u[6] || (u[6] = p("h4", { class: "text-sm font-semibold text-slate-700 dark:text-dark-300" }, "模型配置", -1)),
      (y(!0), w(se, null, Je(e.providers, (d) => (y(), w("div", {
        key: d.name
      }, [
        p("div", {
          class: me([
            "flex items-center justify-between p-3 rounded-lg border cursor-pointer transition-colors",
            d.name === e.activeProviderName ? "border-primary-500 bg-primary-50 dark:bg-primary-900/20" : "border-slate-200 dark:border-dark-600 bg-white dark:bg-dark-800 hover:border-slate-300 dark:hover:border-dark-500"
          ]),
          onClick: (c) => n("setActive", d.name)
        }, [
          p("div", null, [
            p("div", kr, X(d.name), 1),
            p("div", Er, X(d.model), 1)
          ]),
          p("div", Nr, [
            d.name === e.activeProviderName ? (y(), w("span", Sr, "当前")) : re("", !0),
            p("button", {
              class: "text-xs text-red-500 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300",
              onClick: Dn((c) => n("remove", d.name), ["stop"])
            }, " 删除 ", 8, Or)
          ])
        ], 10, wr)
      ]))), 128)),
      p("div", Cr, [
        u[5] || (u[5] = p("h5", { class: "text-xs font-medium text-slate-500 dark:text-dark-400" }, "从预设添加", -1)),
        p("div", Dr, [
          (y(!0), w(se, null, Je(k(r), (d) => (y(), w("button", {
            key: d.name,
            class: "px-2 py-1 text-xs bg-slate-100 dark:bg-dark-700 text-slate-600 dark:text-dark-300 rounded hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors",
            onClick: (c) => i(d)
          }, X(d.name), 9, Ir))), 128))
        ]),
        s.value ? (y(), w("div", Vr, [
          ke(p("input", {
            "onUpdate:modelValue": u[0] || (u[0] = (d) => o.name = d),
            type: "text",
            placeholder: "名称",
            class: "w-full bg-white dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1.5 text-xs text-slate-900 dark:text-white outline-none"
          }, null, 512), [
            [Ee, o.name]
          ]),
          ke(p("input", {
            "onUpdate:modelValue": u[1] || (u[1] = (d) => o.apiKey = d),
            type: "password",
            placeholder: "API Key",
            class: "w-full bg-white dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1.5 text-xs text-slate-900 dark:text-white outline-none"
          }, null, 512), [
            [Ee, o.apiKey]
          ]),
          ke(p("input", {
            "onUpdate:modelValue": u[2] || (u[2] = (d) => o.baseUrl = d),
            type: "text",
            placeholder: "Base URL",
            class: "w-full bg-white dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1.5 text-xs text-slate-900 dark:text-white outline-none"
          }, null, 512), [
            [Ee, o.baseUrl]
          ]),
          ke(p("input", {
            "onUpdate:modelValue": u[3] || (u[3] = (d) => o.model = d),
            type: "text",
            placeholder: "模型名称",
            class: "w-full bg-white dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1.5 text-xs text-slate-900 dark:text-white outline-none"
          }, null, 512), [
            [Ee, o.model]
          ]),
          p("div", Ar, [
            p("button", {
              disabled: !o.name || !o.apiKey,
              class: "px-3 py-1.5 text-xs bg-primary-600 hover:bg-primary-700 disabled:opacity-50 text-white rounded transition-colors",
              onClick: l
            }, " 添加 ", 8, Rr),
            p("button", {
              class: "px-3 py-1.5 text-xs bg-slate-100 dark:bg-dark-700 text-slate-600 dark:text-dark-300 rounded hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors",
              onClick: u[4] || (u[4] = (d) => s.value = null)
            }, " 取消 ")
          ])
        ])) : re("", !0)
      ])
    ]));
  }
}), $r = {
  key: 0,
  class: "fixed inset-0 z-50 flex items-center justify-center p-4"
}, Pr = { class: "relative bg-white dark:bg-dark-800 rounded-xl shadow-2xl border border-slate-200 dark:border-dark-700 w-full max-w-lg" }, Mr = { class: "p-5 space-y-4" }, zr = {
  key: 0,
  class: "p-3 bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 text-sm rounded-lg"
}, jr = {
  key: 1,
  class: "flex items-center justify-center py-8"
}, Kr = { class: "p-3 bg-slate-50 dark:bg-dark-700 rounded-lg text-sm text-slate-600 dark:text-dark-300 whitespace-pre-wrap" }, Hr = { class: "p-3 bg-primary-50 dark:bg-primary-900/20 rounded-lg text-sm text-primary-800 dark:text-primary-200 whitespace-pre-wrap border border-primary-200 dark:border-primary-800" }, Fr = {
  key: 0,
  class: "px-5 py-3 border-t border-slate-100 dark:border-dark-700 flex justify-end gap-2"
}, Ur = ["disabled"], Lr = {
  key: 1,
  class: "px-5 py-3 border-t border-slate-100 dark:border-dark-700 flex justify-end"
}, Wr = /* @__PURE__ */ $e({
  __name: "PromptOptimizeDialog",
  props: {
    show: { type: Boolean },
    optimizing: { type: Boolean },
    original: {},
    optimized: {},
    error: {}
  },
  emits: ["accept", "cancel"],
  setup(e, { emit: t }) {
    const n = t;
    return (r, s) => e.show ? (y(), w("div", $r, [
      p("div", {
        class: "absolute inset-0 bg-black/40 backdrop-blur-sm",
        onClick: s[0] || (s[0] = (o) => n("cancel"))
      }),
      p("div", Pr, [
        s[7] || (s[7] = p("div", { class: "px-5 py-3 border-b border-slate-100 dark:border-dark-700" }, [
          p("h3", { class: "text-base font-semibold text-slate-800 dark:text-white" }, "AI 提示词优化")
        ], -1)),
        p("div", Mr, [
          e.error ? (y(), w("div", zr, X(e.error), 1)) : e.optimizing ? (y(), w("div", jr, [...s[4] || (s[4] = [
            p("div", { class: "flex items-center gap-2 text-slate-500 dark:text-dark-400" }, [
              p("svg", {
                class: "w-5 h-5 animate-spin",
                fill: "none",
                viewBox: "0 0 24 24"
              }, [
                p("circle", {
                  class: "opacity-25",
                  cx: "12",
                  cy: "12",
                  r: "10",
                  stroke: "currentColor",
                  "stroke-width": "4"
                }),
                p("path", {
                  class: "opacity-75",
                  fill: "currentColor",
                  d: "M4 12a8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.969 7.969 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                })
              ]),
              En(" AI 正在优化提示词... ")
            ], -1)
          ])])) : (y(), w(se, { key: 2 }, [
            p("div", null, [
              s[5] || (s[5] = p("label", { class: "block text-xs font-medium text-slate-500 dark:text-dark-400 mb-1" }, "原始提示词", -1)),
              p("div", Kr, X(e.original), 1)
            ]),
            p("div", null, [
              s[6] || (s[6] = p("label", { class: "block text-xs font-medium text-slate-500 dark:text-dark-400 mb-1" }, "优化后提示词", -1)),
              p("div", Hr, X(e.optimized), 1)
            ])
          ], 64))
        ]),
        !e.optimizing && !e.error ? (y(), w("div", Fr, [
          p("button", {
            class: "px-4 py-2 text-sm bg-slate-100 dark:bg-dark-700 text-slate-700 dark:text-dark-300 rounded-lg hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors",
            onClick: s[1] || (s[1] = (o) => n("cancel"))
          }, "取消"),
          p("button", {
            disabled: !e.optimized,
            class: "px-4 py-2 text-sm bg-primary-600 hover:bg-primary-700 disabled:opacity-50 text-white rounded-lg transition-colors",
            onClick: s[2] || (s[2] = (o) => n("accept"))
          }, "采纳并填入终端", 8, Ur)
        ])) : e.error ? (y(), w("div", Lr, [
          p("button", {
            class: "px-4 py-2 text-sm bg-slate-100 dark:bg-dark-700 text-slate-700 dark:text-dark-300 rounded-lg hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors",
            onClick: s[3] || (s[3] = (o) => n("cancel"))
          }, "关闭")
        ])) : re("", !0)
      ])
    ])) : re("", !0);
  }
});
function Br(e, t) {
  const n = /* @__PURE__ */ C([]), r = /* @__PURE__ */ C(""), s = /* @__PURE__ */ C(!1), o = /* @__PURE__ */ C(!1), i = Te(
    () => n.value.find((m) => m.name === r.value)
  ), l = Te(() => n.value.length > 0);
  async function a() {
    s.value = !0;
    try {
      const m = await e("apiProviders");
      if (m) {
        const D = typeof m == "string" ? JSON.parse(m) : m;
        n.value = Array.isArray(D) ? D : [];
      }
      const N = await e("activeProvider");
      r.value = typeof N == "string" ? N : "", !r.value && n.value.length > 0 && (r.value = n.value[0].name);
    } catch (m) {
      console.error("[AI Chatbox] Failed to load config:", m);
    } finally {
      s.value = !1;
    }
  }
  async function u() {
    try {
      await t("apiProviders", JSON.stringify(n.value)), await t("activeProvider", r.value);
    } catch (m) {
      console.error("[AI Chatbox] Failed to save config:", m);
    }
  }
  async function d(m) {
    if (n.value.some((N) => N.name === m.name))
      throw new Error(`Provider "${m.name}" 已存在`);
    n.value.push(m), r.value || (r.value = m.name), await u();
  }
  async function c(m) {
    var N;
    n.value = n.value.filter((D) => D.name !== m), r.value === m && (r.value = ((N = n.value[0]) == null ? void 0 : N.name) || ""), await u();
  }
  async function f(m, N) {
    const D = n.value.findIndex((j) => j.name === m);
    D !== -1 && (n.value[D] = N, r.value === m && (r.value = N.name), await u());
  }
  async function g(m) {
    n.value.some((N) => N.name === m) && (r.value = m, await u());
  }
  async function x(m, N) {
    await d({
      name: m.name,
      apiKey: N,
      baseUrl: m.baseUrl,
      model: m.model
    });
  }
  return {
    providers: n,
    activeProviderName: r,
    activeProvider: i,
    hasProvider: l,
    loading: s,
    showProviderManager: o,
    loadConfig: a,
    addProvider: d,
    removeProvider: c,
    updateProvider: f,
    setActiveProvider: g,
    addFromPreset: x,
    PROVIDER_PRESETS: In
  };
}
function Jr(e) {
  const t = /* @__PURE__ */ C([]), n = /* @__PURE__ */ C(""), r = /* @__PURE__ */ C([]), s = /* @__PURE__ */ C(!1), o = /* @__PURE__ */ C(""), i = /* @__PURE__ */ C(!1), l = Te(
    () => t.value.find((b) => b.id === n.value)
  ), a = Te(() => o.value !== "");
  function u() {
    return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
  }
  async function d() {
    i.value = !0;
    try {
      const b = await e.commands.execute("ai-chatbox.list-conversations", {});
      b && Array.isArray(b) && (t.value = b);
    } catch (b) {
      console.error("[AI Chatbox] Failed to load conversations:", b);
    } finally {
      i.value = !1;
    }
  }
  async function c(b) {
    try {
      const h = await e.commands.execute("ai-chatbox.get-messages", { conversationId: b });
      h && Array.isArray(h) ? r.value = h : r.value = [];
    } catch (h) {
      console.error("[AI Chatbox] Failed to load messages:", h), r.value = [];
    }
    n.value = b;
  }
  async function f(b) {
    try {
      await e.commands.execute("ai-chatbox.save-conversation", { conversation: b });
    } catch (h) {
      console.error("[AI Chatbox] Failed to save conversation:", h);
    }
  }
  async function g(b, h, R, M) {
    try {
      await e.commands.execute("ai-chatbox.save-message", { conversationId: b, role: h, content: R, timestamp: M });
    } catch (Y) {
      console.error("[AI Chatbox] Failed to save message:", Y);
    }
  }
  async function x(b) {
    const h = {
      id: u(),
      title: "新对话",
      createdAt: (/* @__PURE__ */ new Date()).toISOString(),
      updatedAt: (/* @__PURE__ */ new Date()).toISOString(),
      providerName: b
    };
    t.value.unshift(h), await f(h), await c(h.id);
  }
  async function m(b) {
    try {
      await e.commands.execute("ai-chatbox.delete-conversation", { conversationId: b });
    } catch (h) {
      console.error("[AI Chatbox] Failed to delete conversation:", h);
    }
    t.value = t.value.filter((h) => h.id !== b), n.value === b && (n.value = "", r.value = []);
  }
  async function N(b) {
    const h = await e.storage.get("apiProviders"), R = await e.storage.get("activeProvider");
    let M;
    if (h && R)
      try {
        const O = typeof h == "string" ? JSON.parse(h) : h;
        M = (Array.isArray(O) ? O : []).find((Rn) => Rn.name === R);
      } catch {
      }
    if (!M) throw new Error("请先配置 AI 模型");
    n.value || await x(M.name);
    const Y = {
      role: "user",
      content: b,
      timestamp: (/* @__PURE__ */ new Date()).toISOString()
    };
    r.value.push(Y), await g(n.value, "user", b, Y.timestamp);
    const Z = t.value.find((O) => O.id === n.value);
    Z && Z.title === "新对话" && (Z.title = b.slice(0, 30) + (b.length > 30 ? "..." : ""), Z.updatedAt = (/* @__PURE__ */ new Date()).toISOString(), await f(Z)), s.value = !0, o.value = "";
    const Dt = {
      role: "assistant",
      content: "",
      timestamp: (/* @__PURE__ */ new Date()).toISOString()
    };
    r.value.push(Dt);
    const An = r.value.filter((O) => O.content || O.role === "assistant").slice(0, -1).map((O) => ({ role: O.role, content: O.content })), It = u(), et = e.events.on(`ai-chatbox:stream:${It}`, (O) => {
      if (O.chunk) {
        o.value += O.chunk;
        const S = r.value[r.value.length - 1];
        S && S.role === "assistant" && (S.content = o.value);
      } else if (O.done) {
        et.dispose(), s.value = !1, o.value = "";
        const S = r.value[r.value.length - 1];
        S && S.role === "assistant" && g(n.value, "assistant", S.content, S.timestamp), Z && (Z.updatedAt = (/* @__PURE__ */ new Date()).toISOString(), f(Z));
      } else if (O.error) {
        et.dispose(), s.value = !1, o.value = "";
        const S = r.value[r.value.length - 1];
        S && S.role === "assistant" && (S.content = `❌ ${O.error}`), g(n.value, "assistant", (S == null ? void 0 : S.content) || O.error, Dt.timestamp);
      }
    });
    try {
      await e.commands.execute("ai-chatbox.chat-stream", {
        streamId: It,
        provider: M,
        messages: An
      });
    } catch (O) {
      et.dispose(), s.value = !1, o.value = "";
      const S = r.value[r.value.length - 1];
      S && S.role === "assistant" && (S.content = `❌ ${O.message || "请求失败"}`);
    }
  }
  function D() {
    s.value = !1, o.value = "";
  }
  async function j(b) {
    b !== n.value && await c(b);
  }
  return {
    conversations: t,
    currentConvId: n,
    messages: r,
    sending: s,
    isStreaming: a,
    loadingHistory: i,
    currentConversation: l,
    loadConversations: d,
    newConversation: x,
    deleteConversation: m,
    sendMessage: N,
    stopGeneration: D,
    switchConversation: j
  };
}
function Vn(e) {
  const t = /* @__PURE__ */ C(!1), n = /* @__PURE__ */ C(!1), r = /* @__PURE__ */ C(""), s = /* @__PURE__ */ C(""), o = /* @__PURE__ */ C("");
  let i = "";
  function l() {
    return new Promise((c) => {
      const f = e.events.on("ai-chatbox:currentInput", (g) => {
        f.dispose(), c(g);
      });
      e.events.emit("ai-chatbox:getCurrentInput"), setTimeout(() => {
        f.dispose(), c({ sessionId: "", text: "" });
      }, 3e3);
    });
  }
  async function a() {
    const c = await e.storage.get("apiProviders"), f = await e.storage.get("activeProvider");
    let g;
    if (c && f)
      try {
        const m = typeof c == "string" ? JSON.parse(c) : c;
        g = (Array.isArray(m) ? m : []).find((D) => D.name === f);
      } catch {
      }
    if (!g) {
      o.value = "请先配置 AI 模型", n.value = !0;
      return;
    }
    const x = await l();
    if (!x.text) {
      o.value = "终端无输入内容", n.value = !0;
      return;
    }
    i = x.sessionId, r.value = x.text, o.value = "", t.value = !0, n.value = !0, s.value = "";
    try {
      const m = await e.commands.execute("ai-chatbox.optimize-prompt", {
        provider: g,
        prompt: x.text
      });
      s.value = m;
    } catch (m) {
      o.value = m.message || "优化失败";
    } finally {
      t.value = !1;
    }
  }
  async function u() {
    !i || !s.value || (await e.terminal.sendInput(i, "" + s.value), n.value = !1);
  }
  function d() {
    n.value = !1, r.value = "", s.value = "", o.value = "";
  }
  return {
    optimizing: t,
    showDialog: n,
    originalText: r,
    optimizedText: s,
    errorMessage: o,
    optimizePrompt: a,
    acceptOptimized: u,
    cancelOptimize: d
  };
}
const qr = { class: "h-full flex flex-col bg-white dark:bg-dark-900" }, Yr = { class: "px-4 py-2 flex items-center justify-between border-b border-slate-200 dark:border-dark-700 bg-slate-50 dark:bg-dark-800" }, Gr = { class: "flex items-center gap-2" }, Qr = ["value"], Xr = ["value"], Zr = {
  key: 1,
  class: "text-xs text-slate-400"
}, eo = { class: "flex items-center gap-1" }, to = ["disabled"], no = {
  key: 1,
  class: "flex-1 flex flex-col items-center justify-center p-6 text-center"
}, so = {
  key: 0,
  class: "flex flex-col items-center justify-center h-full text-center"
}, ro = { class: "border-t border-slate-200 dark:border-dark-700 p-3" }, oo = /* @__PURE__ */ $e({
  __name: "ChatView",
  setup(e) {
    const t = vn("pluginContext"), n = Br(t.storage.get, t.storage.set), r = Jr(t), s = Vn(t), o = {
      showDialog: s.showDialog,
      optimizing: s.optimizing,
      originalText: s.originalText,
      optimizedText: s.optimizedText,
      errorMessage: s.errorMessage,
      acceptOptimized: s.acceptOptimized,
      cancelOptimize: s.cancelOptimize
    }, i = /* @__PURE__ */ C(null);
    return Ts(() => r.messages.value.length, () => {
      un(() => {
        i.value && (i.value.scrollTop = i.value.scrollHeight);
      });
    }), js(async () => {
      await n.loadConfig(), await r.loadConversations();
    }), (l, a) => (y(), w("div", qr, [
      p("header", Yr, [
        p("div", Gr, [
          k(n).hasProvider.value ? (y(), w("select", {
            key: 0,
            value: k(n).activeProviderName.value,
            class: "bg-white dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1 text-xs text-slate-700 dark:text-white outline-none",
            onChange: a[0] || (a[0] = (u) => k(n).setActiveProvider(u.target.value))
          }, [
            (y(!0), w(se, null, Je(k(n).providers.value, (u) => (y(), w("option", {
              key: u.name,
              value: u.name
            }, X(u.name), 9, Xr))), 128))
          ], 40, Qr)) : (y(), w("span", Zr, "未配置模型"))
        ]),
        p("div", eo, [
          p("button", {
            class: "p-1.5 text-slate-500 dark:text-dark-400 hover:bg-slate-200 dark:hover:bg-dark-700 rounded transition-colors",
            title: "模型配置",
            onClick: a[1] || (a[1] = (u) => k(n).showProviderManager.value = !k(n).showProviderManager.value)
          }, [...a[6] || (a[6] = [
            p("svg", {
              class: "w-4 h-4",
              fill: "none",
              stroke: "currentColor",
              viewBox: "0 0 24 24"
            }, [
              p("path", {
                "stroke-linecap": "round",
                "stroke-linejoin": "round",
                "stroke-width": "2",
                d: "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
              }),
              p("path", {
                "stroke-linecap": "round",
                "stroke-linejoin": "round",
                "stroke-width": "2",
                d: "M15 12a3 3 0 11-6 0 3 3 0 016 0z"
              })
            ], -1)
          ])]),
          p("button", {
            disabled: !k(n).hasProvider.value,
            class: "p-1.5 text-slate-500 dark:text-dark-400 hover:bg-slate-200 dark:hover:bg-dark-700 rounded transition-colors disabled:opacity-50",
            title: "新对话",
            onClick: a[2] || (a[2] = (u) => k(r).newConversation(k(n).activeProviderName.value))
          }, [...a[7] || (a[7] = [
            p("svg", {
              class: "w-4 h-4",
              fill: "none",
              stroke: "currentColor",
              viewBox: "0 0 24 24"
            }, [
              p("path", {
                "stroke-linecap": "round",
                "stroke-linejoin": "round",
                "stroke-width": "2",
                d: "M12 4v16m8-8H4"
              })
            ], -1)
          ])], 8, to)
        ])
      ]),
      k(n).showProviderManager.value ? (y(), mt(Tr, {
        key: 0,
        providers: k(n).providers.value,
        "active-provider-name": k(n).activeProviderName.value,
        onSetActive: k(n).setActiveProvider,
        onRemove: k(n).removeProvider,
        onAdd: k(n).addProvider
      }, null, 8, ["providers", "active-provider-name", "onSetActive", "onRemove", "onAdd"])) : re("", !0),
      k(n).hasProvider.value ? (y(), w(se, { key: 2 }, [
        p("div", {
          ref_key: "messagesContainer",
          ref: i,
          class: "flex-1 overflow-y-auto p-4 space-y-3"
        }, [
          k(r).messages.value.length === 0 ? (y(), w("div", so, [...a[10] || (a[10] = [
            p("div", { class: "text-3xl mb-2" }, "💬", -1),
            p("p", { class: "text-sm text-slate-400 dark:text-dark-500" }, "开始新对话", -1)
          ])])) : re("", !0),
          (y(!0), w(se, null, Je(k(r).messages.value, (u, d) => (y(), mt(mr, {
            key: d,
            message: u,
            streaming: k(r).isStreaming.value && d === k(r).messages.value.length - 1
          }, null, 8, ["message", "streaming"]))), 128))
        ], 512),
        p("div", ro, [
          Ae(_r, {
            disabled: k(r).sending.value || !k(n).activeProvider.value,
            placeholder: "输入消息...",
            onSend: k(r).sendMessage
          }, null, 8, ["disabled", "onSend"])
        ])
      ], 64)) : (y(), w("div", no, [
        a[8] || (a[8] = p("div", { class: "text-4xl mb-3" }, "🤖", -1)),
        a[9] || (a[9] = p("p", { class: "text-sm text-slate-500 dark:text-dark-400 mb-3" }, "请先配置 AI 模型", -1)),
        p("button", {
          class: "px-4 py-2 text-sm bg-primary-600 hover:bg-primary-700 text-white rounded-lg transition-colors",
          onClick: a[3] || (a[3] = (u) => k(n).showProviderManager.value = !0)
        }, " 配置模型 ")
      ])),
      Ae(Wr, {
        show: o.showDialog.value,
        optimizing: o.optimizing.value,
        original: o.originalText.value,
        optimized: o.optimizedText.value,
        error: o.errorMessage.value,
        onAccept: a[4] || (a[4] = (u) => o.acceptOptimized()),
        onCancel: a[5] || (a[5] = (u) => o.cancelOptimize())
      }, null, 8, ["show", "optimizing", "original", "optimized", "error"])
    ]));
  }
});
async function io(e) {
  e.ui.registerSidebarPanel({
    id: "ai-chatbox.sidebar",
    title: "AI 对话",
    component: oo
  });
  const t = Vn(e);
  e.ui.registerTerminalToolbarItem({
    id: "ai-optimize-prompt",
    label: "AI 优化",
    icon: "✨",
    onClick: () => t.optimizePrompt()
  }), console.log("[AI Chatbox] Plugin activated (rust-ts mode)");
}
async function ao() {
  console.log("[AI Chatbox] Plugin deactivated");
}
export {
  io as activate,
  ao as deactivate
};

/**
* @vue/shared v3.5.33
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
}, An = (e) => e.charCodeAt(0) === 111 && e.charCodeAt(1) === 110 && // uppercase letter
(e.charCodeAt(2) > 122 || e.charCodeAt(2) < 97), Rn = (e) => e.startsWith("onUpdate:"), B = Object.assign, Vn = Object.prototype.hasOwnProperty, lt = (e, t) => Vn.call(e, t), O = Array.isArray, oe = (e) => Ge(e) === "[object Map]", zt = (e) => Ge(e) === "[object Set]", D = (e) => typeof e == "function", H = (e) => typeof e == "string", ce = (e) => typeof e == "symbol", T = (e) => e !== null && typeof e == "object", Pn = (e) => (T(e) || D(e)) && D(e.then) && D(e.catch), Kt = Object.prototype.toString, Ge = (e) => Kt.call(e), Ht = (e) => Ge(e).slice(8, -1), Ft = (e) => Ge(e) === "[object Object]", vt = (e) => H(e) && e !== "NaN" && e[0] !== "-" && "" + parseInt(e, 10) === e, yt = (e) => {
  const t = /* @__PURE__ */ Object.create(null);
  return (n) => t[n] || (t[n] = e(n));
}, Mn = /\B([A-Z])/g, jn = yt(
  (e) => e.replace(Mn, "-$1").toLowerCase()
), Ut = yt((e) => e.charAt(0).toUpperCase() + e.slice(1)), zn = yt(
  (e) => e ? `on${Ut(e)}` : ""
), W = (e, t) => !Object.is(e, t), Kn = (e, ...t) => {
  for (let n = 0; n < e.length; n++)
    e[n](...t);
}, Lt = (e) => {
  const t = parseFloat(e);
  return isNaN(t) ? e : t;
};
let Dt;
const Qe = () => Dt || (Dt = typeof globalThis < "u" ? globalThis : typeof self < "u" ? self : typeof window < "u" ? window : typeof global < "u" ? global : {});
function bt(e) {
  if (O(e)) {
    const t = {};
    for (let n = 0; n < e.length; n++) {
      const s = e[n], r = H(s) ? Ln(s) : bt(s);
      if (r)
        for (const o in r)
          t[o] = r[o];
    }
    return t;
  } else if (H(e) || T(e))
    return e;
}
const Hn = /;(?![^(]*\))/g, Fn = /:([^]+)/, Un = /\/\*[^]*?\*\//g;
function Ln(e) {
  const t = {};
  return e.replace(Un, "").split(Hn).forEach((n) => {
    if (n) {
      const s = n.split(Fn);
      s.length > 1 && (t[s[0].trim()] = s[1].trim());
    }
  }), t;
}
function me(e) {
  let t = "";
  if (H(e))
    t = e;
  else if (O(e))
    for (let n = 0; n < e.length; n++) {
      const s = me(e[n]);
      s && (t += s + " ");
    }
  else if (T(e))
    for (const n in e)
      e[n] && (t += n + " ");
  return t.trim();
}
const Wt = (e) => !!(e && e.__v_isRef === !0), G = (e) => H(e) ? e : e == null ? "" : O(e) || T(e) && (e.toString === Kt || !D(e.toString)) ? Wt(e) ? G(e.value) : JSON.stringify(e, Bt, 2) : String(e), Bt = (e, t) => Wt(t) ? Bt(e, t.value) : oe(t) ? {
  [`Map(${t.size})`]: [...t.entries()].reduce(
    (n, [s, r], o) => (n[et(s, o) + " =>"] = r, n),
    {}
  )
} : zt(t) ? {
  [`Set(${t.size})`]: [...t.values()].map((n) => et(n))
} : ce(t) ? et(t) : T(t) && !O(t) && !Ft(t) ? String(t) : t, et = (e, t = "") => {
  var n;
  return (
    // Symbol.description in es2019+ so we need to cast here to pass
    // the lib: es2016 check
    ce(e) ? `Symbol(${(n = e.description) != null ? n : t})` : e
  );
};
/**
* @vue/reactivity v3.5.33
* (c) 2018-present Yuxi (Evan) You and Vue contributors
* @license MIT
**/
function J(e, ...t) {
  console.warn(`[Vue warn] ${e}`, ...t);
}
let x;
const tt = /* @__PURE__ */ new WeakSet();
class Wn {
  constructor(t) {
    this.fn = t, this.deps = void 0, this.depsTail = void 0, this.flags = 5, this.next = void 0, this.cleanup = void 0, this.scheduler = void 0;
  }
  pause() {
    this.flags |= 64;
  }
  resume() {
    this.flags & 64 && (this.flags &= -65, tt.has(this) && (tt.delete(this), this.trigger()));
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
    this.flags |= 2, It(this), qt(this);
    const t = x, n = K;
    x = this, K = !0;
    try {
      return this.fn();
    } finally {
      process.env.NODE_ENV !== "production" && x !== this && J(
        "Active effect was not restored correctly - this is likely a Vue internal bug."
      ), Gt(this), x = t, K = n, this.flags &= -3;
    }
  }
  stop() {
    if (this.flags & 1) {
      for (let t = this.deps; t; t = t.nextDep)
        xt(t);
      this.deps = this.depsTail = void 0, It(this), this.onStop && this.onStop(), this.flags &= -2;
    }
  }
  trigger() {
    this.flags & 64 ? tt.add(this) : this.scheduler ? this.scheduler() : this.runIfDirty();
  }
  /**
   * @internal
   */
  runIfDirty() {
    ct(this) && this.run();
  }
  get dirty() {
    return ct(this);
  }
}
let Jt = 0, Se, Ne;
function Yt(e, t = !1) {
  if (e.flags |= 8, t) {
    e.next = Ne, Ne = e;
    return;
  }
  e.next = Se, Se = e;
}
function _t() {
  Jt++;
}
function wt() {
  if (--Jt > 0)
    return;
  if (Ne) {
    let t = Ne;
    for (Ne = void 0; t; ) {
      const n = t.next;
      t.next = void 0, t.flags &= -9, t = n;
    }
  }
  let e;
  for (; Se; ) {
    let t = Se;
    for (Se = void 0; t; ) {
      const n = t.next;
      if (t.next = void 0, t.flags &= -9, t.flags & 1)
        try {
          t.trigger();
        } catch (s) {
          e || (e = s);
        }
      t = n;
    }
  }
  if (e) throw e;
}
function qt(e) {
  for (let t = e.deps; t; t = t.nextDep)
    t.version = -1, t.prevActiveLink = t.dep.activeLink, t.dep.activeLink = t;
}
function Gt(e) {
  let t, n = e.depsTail, s = n;
  for (; s; ) {
    const r = s.prevDep;
    s.version === -1 ? (s === n && (n = r), xt(s), Bn(s)) : t = s, s.dep.activeLink = s.prevActiveLink, s.prevActiveLink = void 0, s = r;
  }
  e.deps = t, e.depsTail = n;
}
function ct(e) {
  for (let t = e.deps; t; t = t.nextDep)
    if (t.dep.version !== t.version || t.dep.computed && (Qt(t.dep.computed) || t.dep.version !== t.version))
      return !0;
  return !!e._dirty;
}
function Qt(e) {
  if (e.flags & 4 && !(e.flags & 16) || (e.flags &= -17, e.globalVersion === Ce) || (e.globalVersion = Ce, !e.isSSR && e.flags & 128 && (!e.deps && !e._dirty || !ct(e))))
    return;
  e.flags |= 2;
  const t = e.dep, n = x, s = K;
  x = e, K = !0;
  try {
    qt(e);
    const r = e.fn(e._value);
    (t.version === 0 || W(r, e._value)) && (e.flags |= 128, e._value = r, t.version++);
  } catch (r) {
    throw t.version++, r;
  } finally {
    x = n, K = s, Gt(e), e.flags &= -3;
  }
}
function xt(e, t = !1) {
  const { dep: n, prevSub: s, nextSub: r } = e;
  if (s && (s.nextSub = r, e.prevSub = void 0), r && (r.prevSub = s, e.nextSub = void 0), process.env.NODE_ENV !== "production" && n.subsHead === e && (n.subsHead = r), n.subs === e && (n.subs = s, !s && n.computed)) {
    n.computed.flags &= -5;
    for (let o = n.computed.deps; o; o = o.nextDep)
      xt(o, !0);
  }
  !t && !--n.sc && n.map && n.map.delete(n.key);
}
function Bn(e) {
  const { prevDep: t, nextDep: n } = e;
  t && (t.nextDep = n, e.prevDep = void 0), n && (n.prevDep = t, e.nextDep = void 0);
}
let K = !0;
const Zt = [];
function ye() {
  Zt.push(K), K = !1;
}
function be() {
  const e = Zt.pop();
  K = e === void 0 ? !0 : e;
}
function It(e) {
  const { cleanup: t } = e;
  if (e.cleanup = void 0, t) {
    const n = x;
    x = void 0;
    try {
      t();
    } finally {
      x = n;
    }
  }
}
let Ce = 0;
class Jn {
  constructor(t, n) {
    this.sub = t, this.dep = n, this.version = n.version, this.nextDep = this.prevDep = this.nextSub = this.prevSub = this.prevActiveLink = void 0;
  }
}
class kt {
  // TODO isolatedDeclarations "__v_skip"
  constructor(t) {
    this.computed = t, this.version = 0, this.activeLink = void 0, this.subs = void 0, this.map = void 0, this.key = void 0, this.sc = 0, this.__v_skip = !0, process.env.NODE_ENV !== "production" && (this.subsHead = void 0);
  }
  track(t) {
    if (!x || !K || x === this.computed)
      return;
    let n = this.activeLink;
    if (n === void 0 || n.sub !== x)
      n = this.activeLink = new Jn(x, this), x.deps ? (n.prevDep = x.depsTail, x.depsTail.nextDep = n, x.depsTail = n) : x.deps = x.depsTail = n, Xt(n);
    else if (n.version === -1 && (n.version = this.version, n.nextDep)) {
      const s = n.nextDep;
      s.prevDep = n.prevDep, n.prevDep && (n.prevDep.nextDep = s), n.prevDep = x.depsTail, n.nextDep = void 0, x.depsTail.nextDep = n, x.depsTail = n, x.deps === n && (x.deps = s);
    }
    return process.env.NODE_ENV !== "production" && x.onTrack && x.onTrack(
      B(
        {
          effect: x
        },
        t
      )
    ), n;
  }
  trigger(t) {
    this.version++, Ce++, this.notify(t);
  }
  notify(t) {
    _t();
    try {
      if (process.env.NODE_ENV !== "production")
        for (let n = this.subsHead; n; n = n.nextSub)
          n.sub.onTrigger && !(n.sub.flags & 8) && n.sub.onTrigger(
            B(
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
function Xt(e) {
  if (e.dep.sc++, e.sub.flags & 4) {
    const t = e.dep.computed;
    if (t && !e.dep.subs) {
      t.flags |= 20;
      for (let s = t.deps; s; s = s.nextDep)
        Xt(s);
    }
    const n = e.dep.subs;
    n !== e && (e.prevSub = n, n && (n.nextSub = e)), process.env.NODE_ENV !== "production" && e.dep.subsHead === void 0 && (e.dep.subsHead = e), e.dep.subs = e;
  }
}
const ut = /* @__PURE__ */ new WeakMap(), ie = /* @__PURE__ */ Symbol(
  process.env.NODE_ENV !== "production" ? "Object iterate" : ""
), dt = /* @__PURE__ */ Symbol(
  process.env.NODE_ENV !== "production" ? "Map keys iterate" : ""
), De = /* @__PURE__ */ Symbol(
  process.env.NODE_ENV !== "production" ? "Array iterate" : ""
);
function A(e, t, n) {
  if (K && x) {
    let s = ut.get(e);
    s || ut.set(e, s = /* @__PURE__ */ new Map());
    let r = s.get(n);
    r || (s.set(n, r = new kt()), r.map = s, r.key = n), process.env.NODE_ENV !== "production" ? r.track({
      target: e,
      type: t,
      key: n
    }) : r.track();
  }
}
function Z(e, t, n, s, r, o) {
  const a = ut.get(e);
  if (!a) {
    Ce++;
    return;
  }
  const i = (l) => {
    l && (process.env.NODE_ENV !== "production" ? l.trigger({
      target: e,
      type: t,
      key: n,
      newValue: s,
      oldValue: r,
      oldTarget: o
    }) : l.trigger());
  };
  if (_t(), t === "clear")
    a.forEach(i);
  else {
    const l = O(e), u = l && vt(n);
    if (l && n === "length") {
      const d = Number(s);
      a.forEach((c, f) => {
        (f === "length" || f === De || !ce(f) && f >= d) && i(c);
      });
    } else
      switch ((n !== void 0 || a.has(void 0)) && i(a.get(n)), u && i(a.get(De)), t) {
        case "add":
          l ? u && i(a.get("length")) : (i(a.get(ie)), oe(e) && i(a.get(dt)));
          break;
        case "delete":
          l || (i(a.get(ie)), oe(e) && i(a.get(dt)));
          break;
        case "set":
          oe(e) && i(a.get(ie));
          break;
      }
  }
  wt();
}
function de(e) {
  const t = /* @__PURE__ */ m(e);
  return t === e ? t : (A(t, "iterate", De), /* @__PURE__ */ $(e) ? t : t.map(U));
}
function Ze(e) {
  return A(e = /* @__PURE__ */ m(e), "iterate", De), e;
}
function L(e, t) {
  return /* @__PURE__ */ F(e) ? ve(/* @__PURE__ */ ae(e) ? U(t) : t) : U(t);
}
const Yn = {
  __proto__: null,
  [Symbol.iterator]() {
    return nt(this, Symbol.iterator, (e) => L(this, e));
  },
  concat(...e) {
    return de(this).concat(
      ...e.map((t) => O(t) ? de(t) : t)
    );
  },
  entries() {
    return nt(this, "entries", (e) => (e[1] = L(this, e[1]), e));
  },
  every(e, t) {
    return Y(this, "every", e, t, void 0, arguments);
  },
  filter(e, t) {
    return Y(
      this,
      "filter",
      e,
      t,
      (n) => n.map((s) => L(this, s)),
      arguments
    );
  },
  find(e, t) {
    return Y(
      this,
      "find",
      e,
      t,
      (n) => L(this, n),
      arguments
    );
  },
  findIndex(e, t) {
    return Y(this, "findIndex", e, t, void 0, arguments);
  },
  findLast(e, t) {
    return Y(
      this,
      "findLast",
      e,
      t,
      (n) => L(this, n),
      arguments
    );
  },
  findLastIndex(e, t) {
    return Y(this, "findLastIndex", e, t, void 0, arguments);
  },
  // flat, flatMap could benefit from ARRAY_ITERATE but are not straight-forward to implement
  forEach(e, t) {
    return Y(this, "forEach", e, t, void 0, arguments);
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
    return Y(this, "map", e, t, void 0, arguments);
  },
  pop() {
    return we(this, "pop");
  },
  push(...e) {
    return we(this, "push", e);
  },
  reduce(e, ...t) {
    return Tt(this, "reduce", e, t);
  },
  reduceRight(e, ...t) {
    return Tt(this, "reduceRight", e, t);
  },
  shift() {
    return we(this, "shift");
  },
  // slice could use ARRAY_ITERATE but also seems to beg for range tracking
  some(e, t) {
    return Y(this, "some", e, t, void 0, arguments);
  },
  splice(...e) {
    return we(this, "splice", e);
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
    return we(this, "unshift", e);
  },
  values() {
    return nt(this, "values", (e) => L(this, e));
  }
};
function nt(e, t, n) {
  const s = Ze(e), r = s[t]();
  return s !== e && !/* @__PURE__ */ $(e) && (r._next = r.next, r.next = () => {
    const o = r._next();
    return o.done || (o.value = n(o.value)), o;
  }), r;
}
const qn = Array.prototype;
function Y(e, t, n, s, r, o) {
  const a = Ze(e), i = a !== e && !/* @__PURE__ */ $(e), l = a[t];
  if (l !== qn[t]) {
    const c = l.apply(e, o);
    return i ? U(c) : c;
  }
  let u = n;
  a !== e && (i ? u = function(c, f) {
    return n.call(this, L(e, c), f, e);
  } : n.length > 2 && (u = function(c, f) {
    return n.call(this, c, f, e);
  }));
  const d = l.call(a, u, s);
  return i && r ? r(d) : d;
}
function Tt(e, t, n, s) {
  const r = Ze(e), o = r !== e && !/* @__PURE__ */ $(e);
  let a = n, i = !1;
  r !== e && (o ? (i = s.length === 0, a = function(u, d, c) {
    return i && (i = !1, u = L(e, u)), n.call(this, u, L(e, d), c, e);
  }) : n.length > 3 && (a = function(u, d, c) {
    return n.call(this, u, d, c, e);
  }));
  const l = r[t](a, ...s);
  return i ? L(e, l) : l;
}
function rt(e, t, n) {
  const s = /* @__PURE__ */ m(e);
  A(s, "iterate", De);
  const r = s[t](...n);
  return (r === -1 || r === !1) && /* @__PURE__ */ Ue(n[0]) ? (n[0] = /* @__PURE__ */ m(n[0]), s[t](...n)) : r;
}
function we(e, t, n = []) {
  ye(), _t();
  const s = (/* @__PURE__ */ m(e))[t].apply(e, n);
  return wt(), be(), s;
}
const Gn = /* @__PURE__ */ Tn("__proto__,__v_isRef,__isVue"), en = new Set(
  /* @__PURE__ */ Object.getOwnPropertyNames(Symbol).filter((e) => e !== "arguments" && e !== "caller").map((e) => Symbol[e]).filter(ce)
);
function Qn(e) {
  ce(e) || (e = String(e));
  const t = /* @__PURE__ */ m(this);
  return A(t, "has", e), t.hasOwnProperty(e);
}
class tn {
  constructor(t = !1, n = !1) {
    this._isReadonly = t, this._isShallow = n;
  }
  get(t, n, s) {
    if (n === "__v_skip") return t.__v_skip;
    const r = this._isReadonly, o = this._isShallow;
    if (n === "__v_isReactive")
      return !r;
    if (n === "__v_isReadonly")
      return r;
    if (n === "__v_isShallow")
      return o;
    if (n === "__v_raw")
      return s === (r ? o ? ar : sn : o ? ir : rn).get(t) || // receiver is not the reactive proxy, but has the same prototype
      // this means the receiver is a user proxy of the reactive proxy
      Object.getPrototypeOf(t) === Object.getPrototypeOf(s) ? t : void 0;
    const a = O(t);
    if (!r) {
      let l;
      if (a && (l = Yn[n]))
        return l;
      if (n === "hasOwnProperty")
        return Qn;
    }
    const i = Reflect.get(
      t,
      n,
      // if this is a proxy wrapping a ref, return methods using the raw ref
      // as receiver so that we don't have to call `toRaw` on the ref in all
      // its class methods
      /* @__PURE__ */ R(t) ? t : s
    );
    if ((ce(n) ? en.has(n) : Gn(n)) || (r || A(t, "get", n), o))
      return i;
    if (/* @__PURE__ */ R(i)) {
      const l = a && vt(n) ? i : i.value;
      return r && T(l) ? /* @__PURE__ */ pt(l) : l;
    }
    return T(i) ? r ? /* @__PURE__ */ pt(i) : /* @__PURE__ */ Et(i) : i;
  }
}
class Zn extends tn {
  constructor(t = !1) {
    super(!1, t);
  }
  set(t, n, s, r) {
    let o = t[n];
    const a = O(t) && vt(n);
    if (!this._isShallow) {
      const u = /* @__PURE__ */ F(o);
      if (!/* @__PURE__ */ $(s) && !/* @__PURE__ */ F(s) && (o = /* @__PURE__ */ m(o), s = /* @__PURE__ */ m(s)), !a && /* @__PURE__ */ R(o) && !/* @__PURE__ */ R(s))
        return u ? (process.env.NODE_ENV !== "production" && J(
          `Set operation on key "${String(n)}" failed: target is readonly.`,
          t[n]
        ), !0) : (o.value = s, !0);
    }
    const i = a ? Number(n) < t.length : lt(t, n), l = Reflect.set(
      t,
      n,
      s,
      /* @__PURE__ */ R(t) ? t : r
    );
    return t === /* @__PURE__ */ m(r) && (i ? W(s, o) && Z(t, "set", n, s, o) : Z(t, "add", n, s)), l;
  }
  deleteProperty(t, n) {
    const s = lt(t, n), r = t[n], o = Reflect.deleteProperty(t, n);
    return o && s && Z(t, "delete", n, void 0, r), o;
  }
  has(t, n) {
    const s = Reflect.has(t, n);
    return (!ce(n) || !en.has(n)) && A(t, "has", n), s;
  }
  ownKeys(t) {
    return A(
      t,
      "iterate",
      O(t) ? "length" : ie
    ), Reflect.ownKeys(t);
  }
}
class Xn extends tn {
  constructor(t = !1) {
    super(!0, t);
  }
  set(t, n) {
    return process.env.NODE_ENV !== "production" && J(
      `Set operation on key "${String(n)}" failed: target is readonly.`,
      t
    ), !0;
  }
  deleteProperty(t, n) {
    return process.env.NODE_ENV !== "production" && J(
      `Delete operation on key "${String(n)}" failed: target is readonly.`,
      t
    ), !0;
  }
}
const er = /* @__PURE__ */ new Zn(), tr = /* @__PURE__ */ new Xn(), ft = (e) => e, Me = (e) => Reflect.getPrototypeOf(e);
function nr(e, t, n) {
  return function(...s) {
    const r = this.__v_raw, o = /* @__PURE__ */ m(r), a = oe(o), i = e === "entries" || e === Symbol.iterator && a, l = e === "keys" && a, u = r[e](...s), d = n ? ft : t ? ve : U;
    return !t && A(
      o,
      "iterate",
      l ? dt : ie
    ), B(
      // inheriting all iterator properties
      Object.create(u),
      {
        // iterator protocol
        next() {
          const { value: c, done: f } = u.next();
          return f ? { value: c, done: f } : {
            value: i ? [d(c[0]), d(c[1])] : d(c),
            done: f
          };
        }
      }
    );
  };
}
function je(e) {
  return function(...t) {
    if (process.env.NODE_ENV !== "production") {
      const n = t[0] ? `on key "${t[0]}" ` : "";
      J(
        `${Ut(e)} operation ${n}failed: target is readonly.`,
        /* @__PURE__ */ m(this)
      );
    }
    return e === "delete" ? !1 : e === "clear" ? void 0 : this;
  };
}
function rr(e, t) {
  const n = {
    get(r) {
      const o = this.__v_raw, a = /* @__PURE__ */ m(o), i = /* @__PURE__ */ m(r);
      e || (W(r, i) && A(a, "get", r), A(a, "get", i));
      const { has: l } = Me(a), u = t ? ft : e ? ve : U;
      if (l.call(a, r))
        return u(o.get(r));
      if (l.call(a, i))
        return u(o.get(i));
      o !== a && o.get(r);
    },
    get size() {
      const r = this.__v_raw;
      return !e && A(/* @__PURE__ */ m(r), "iterate", ie), r.size;
    },
    has(r) {
      const o = this.__v_raw, a = /* @__PURE__ */ m(o), i = /* @__PURE__ */ m(r);
      return e || (W(r, i) && A(a, "has", r), A(a, "has", i)), r === i ? o.has(r) : o.has(r) || o.has(i);
    },
    forEach(r, o) {
      const a = this, i = a.__v_raw, l = /* @__PURE__ */ m(i), u = t ? ft : e ? ve : U;
      return !e && A(l, "iterate", ie), i.forEach((d, c) => r.call(o, u(d), u(c), a));
    }
  };
  return B(
    n,
    e ? {
      add: je("add"),
      set: je("set"),
      delete: je("delete"),
      clear: je("clear")
    } : {
      add(r) {
        const o = /* @__PURE__ */ m(this), a = Me(o), i = /* @__PURE__ */ m(r), l = !t && !/* @__PURE__ */ $(r) && !/* @__PURE__ */ F(r) ? i : r;
        return a.has.call(o, l) || W(r, l) && a.has.call(o, r) || W(i, l) && a.has.call(o, i) || (o.add(l), Z(o, "add", l, l)), this;
      },
      set(r, o) {
        !t && !/* @__PURE__ */ $(o) && !/* @__PURE__ */ F(o) && (o = /* @__PURE__ */ m(o));
        const a = /* @__PURE__ */ m(this), { has: i, get: l } = Me(a);
        let u = i.call(a, r);
        u ? process.env.NODE_ENV !== "production" && $t(a, i, r) : (r = /* @__PURE__ */ m(r), u = i.call(a, r));
        const d = l.call(a, r);
        return a.set(r, o), u ? W(o, d) && Z(a, "set", r, o, d) : Z(a, "add", r, o), this;
      },
      delete(r) {
        const o = /* @__PURE__ */ m(this), { has: a, get: i } = Me(o);
        let l = a.call(o, r);
        l ? process.env.NODE_ENV !== "production" && $t(o, a, r) : (r = /* @__PURE__ */ m(r), l = a.call(o, r));
        const u = i ? i.call(o, r) : void 0, d = o.delete(r);
        return l && Z(o, "delete", r, void 0, u), d;
      },
      clear() {
        const r = /* @__PURE__ */ m(this), o = r.size !== 0, a = process.env.NODE_ENV !== "production" ? oe(r) ? new Map(r) : new Set(r) : void 0, i = r.clear();
        return o && Z(
          r,
          "clear",
          void 0,
          void 0,
          a
        ), i;
      }
    }
  ), [
    "keys",
    "values",
    "entries",
    Symbol.iterator
  ].forEach((r) => {
    n[r] = nr(r, e, t);
  }), n;
}
function nn(e, t) {
  const n = rr(e, t);
  return (s, r, o) => r === "__v_isReactive" ? !e : r === "__v_isReadonly" ? e : r === "__v_raw" ? s : Reflect.get(
    lt(n, r) && r in s ? n : s,
    r,
    o
  );
}
const sr = {
  get: /* @__PURE__ */ nn(!1, !1)
}, or = {
  get: /* @__PURE__ */ nn(!0, !1)
};
function $t(e, t, n) {
  const s = /* @__PURE__ */ m(n);
  if (s !== n && t.call(e, s)) {
    const r = Ht(e);
    J(
      `Reactive ${r} contains both the raw and reactive versions of the same object${r === "Map" ? " as keys" : ""}, which can lead to inconsistencies. Avoid differentiating between the raw and reactive versions of an object and only use the reactive version if possible.`
    );
  }
}
const rn = /* @__PURE__ */ new WeakMap(), ir = /* @__PURE__ */ new WeakMap(), sn = /* @__PURE__ */ new WeakMap(), ar = /* @__PURE__ */ new WeakMap();
function lr(e) {
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
function cr(e) {
  return e.__v_skip || !Object.isExtensible(e) ? 0 : lr(Ht(e));
}
// @__NO_SIDE_EFFECTS__
function Et(e) {
  return /* @__PURE__ */ F(e) ? e : on(
    e,
    !1,
    er,
    sr,
    rn
  );
}
// @__NO_SIDE_EFFECTS__
function pt(e) {
  return on(
    e,
    !0,
    tr,
    or,
    sn
  );
}
function on(e, t, n, s, r) {
  if (!T(e))
    return process.env.NODE_ENV !== "production" && J(
      `value cannot be made ${t ? "readonly" : "reactive"}: ${String(
        e
      )}`
    ), e;
  if (e.__v_raw && !(t && e.__v_isReactive))
    return e;
  const o = cr(e);
  if (o === 0)
    return e;
  const a = r.get(e);
  if (a)
    return a;
  const i = new Proxy(
    e,
    o === 2 ? s : n
  );
  return r.set(e, i), i;
}
// @__NO_SIDE_EFFECTS__
function ae(e) {
  return /* @__PURE__ */ F(e) ? /* @__PURE__ */ ae(e.__v_raw) : !!(e && e.__v_isReactive);
}
// @__NO_SIDE_EFFECTS__
function F(e) {
  return !!(e && e.__v_isReadonly);
}
// @__NO_SIDE_EFFECTS__
function $(e) {
  return !!(e && e.__v_isShallow);
}
// @__NO_SIDE_EFFECTS__
function Ue(e) {
  return e ? !!e.__v_raw : !1;
}
// @__NO_SIDE_EFFECTS__
function m(e) {
  const t = e && e.__v_raw;
  return t ? /* @__PURE__ */ m(t) : e;
}
const U = (e) => T(e) ? /* @__PURE__ */ Et(e) : e, ve = (e) => T(e) ? /* @__PURE__ */ pt(e) : e;
// @__NO_SIDE_EFFECTS__
function R(e) {
  return e ? e.__v_isRef === !0 : !1;
}
// @__NO_SIDE_EFFECTS__
function N(e) {
  return ur(e, !1);
}
function ur(e, t) {
  return /* @__PURE__ */ R(e) ? e : new dr(e, t);
}
class dr {
  constructor(t, n) {
    this.dep = new kt(), this.__v_isRef = !0, this.__v_isShallow = !1, this._rawValue = n ? t : /* @__PURE__ */ m(t), this._value = n ? t : U(t), this.__v_isShallow = n;
  }
  get value() {
    return process.env.NODE_ENV !== "production" ? this.dep.track({
      target: this,
      type: "get",
      key: "value"
    }) : this.dep.track(), this._value;
  }
  set value(t) {
    const n = this._rawValue, s = this.__v_isShallow || /* @__PURE__ */ $(t) || /* @__PURE__ */ F(t);
    t = s ? t : /* @__PURE__ */ m(t), W(t, n) && (this._rawValue = t, this._value = s ? t : U(t), process.env.NODE_ENV !== "production" ? this.dep.trigger({
      target: this,
      type: "set",
      key: "value",
      newValue: t,
      oldValue: n
    }) : this.dep.trigger());
  }
}
function _(e) {
  return /* @__PURE__ */ R(e) ? e.value : e;
}
class fr {
  constructor(t, n, s) {
    this.fn = t, this.setter = n, this._value = void 0, this.dep = new kt(this), this.__v_isRef = !0, this.deps = void 0, this.depsTail = void 0, this.flags = 16, this.globalVersion = Ce - 1, this.next = void 0, this.effect = this, this.__v_isReadonly = !n, this.isSSR = s;
  }
  /**
   * @internal
   */
  notify() {
    if (this.flags |= 16, !(this.flags & 8) && // avoid infinite self recursion
    x !== this)
      return Yt(this, !0), !0;
    process.env.NODE_ENV;
  }
  get value() {
    const t = process.env.NODE_ENV !== "production" ? this.dep.track({
      target: this,
      type: "get",
      key: "value"
    }) : this.dep.track();
    return Qt(this), t && (t.version = this.dep.version), this._value;
  }
  set value(t) {
    this.setter ? this.setter(t) : process.env.NODE_ENV !== "production" && J("Write operation failed: computed value is readonly");
  }
}
// @__NO_SIDE_EFFECTS__
function pr(e, t, n = !1) {
  let s, r;
  D(e) ? s = e : (s = e.get, r = e.set);
  const o = new fr(s, r, n);
  return process.env.NODE_ENV, o;
}
const ze = {}, Le = /* @__PURE__ */ new WeakMap();
let se;
function hr(e, t = !1, n = se) {
  if (n) {
    let s = Le.get(n);
    s || Le.set(n, s = []), s.push(e);
  } else process.env.NODE_ENV !== "production" && !t && J(
    "onWatcherCleanup() was called when there was no active watcher to associate with."
  );
}
function gr(e, t, n = Oe) {
  const { immediate: s, deep: r, once: o, scheduler: a, augmentJob: i, call: l } = n, u = (v) => {
    (n.onWarn || J)(
      "Invalid watch source: ",
      v,
      "A watch source can only be a getter/effect function, a ref, a reactive object, or an array of these types."
    );
  }, d = (v) => r ? v : /* @__PURE__ */ $(v) || r === !1 || r === 0 ? X(v, 1) : X(v);
  let c, f, g, y, h = !1, k = !1;
  if (/* @__PURE__ */ R(e) ? (f = () => e.value, h = /* @__PURE__ */ $(e)) : /* @__PURE__ */ ae(e) ? (f = () => d(e), h = !0) : O(e) ? (k = !0, h = e.some((v) => /* @__PURE__ */ ae(v) || /* @__PURE__ */ $(v)), f = () => e.map((v) => {
    if (/* @__PURE__ */ R(v))
      return v.value;
    if (/* @__PURE__ */ ae(v))
      return d(v);
    if (D(v))
      return l ? l(v, 2) : v();
    process.env.NODE_ENV !== "production" && u(v);
  })) : D(e) ? t ? f = l ? () => l(e, 2) : e : f = () => {
    if (g) {
      ye();
      try {
        g();
      } finally {
        be();
      }
    }
    const v = se;
    se = c;
    try {
      return l ? l(e, 3, [y]) : e(y);
    } finally {
      se = v;
    }
  } : (f = he, process.env.NODE_ENV !== "production" && u(e)), t && r) {
    const v = f, V = r === !0 ? 1 / 0 : r;
    f = () => X(v(), V);
  }
  const I = () => {
    c.stop();
  };
  if (o && t) {
    const v = t;
    t = (...V) => {
      v(...V), I();
    };
  }
  let M = k ? new Array(e.length).fill(ze) : ze;
  const ne = (v) => {
    if (!(!(c.flags & 1) || !c.dirty && !v))
      if (t) {
        const V = c.run();
        if (r || h || (k ? V.some((b, S) => W(b, M[S])) : W(V, M))) {
          g && g();
          const b = se;
          se = c;
          try {
            const S = [
              V,
              // pass undefined as the old value when it's changed for the first time
              M === ze ? void 0 : k && M[0] === ze ? [] : M,
              y
            ];
            M = V, l ? l(t, 3, S) : (
              // @ts-expect-error
              t(...S)
            );
          } finally {
            se = b;
          }
        }
      } else
        c.run();
  };
  return i && i(ne), c = new Wn(f), c.scheduler = a ? () => a(ne, !1) : ne, y = (v) => hr(v, !1, c), g = c.onStop = () => {
    const v = Le.get(c);
    if (v) {
      if (l)
        l(v, 4);
      else
        for (const V of v) V();
      Le.delete(c);
    }
  }, process.env.NODE_ENV !== "production" && (c.onTrack = n.onTrack, c.onTrigger = n.onTrigger), t ? s ? ne(!0) : M = c.run() : a ? a(ne.bind(null, !0), !0) : c.run(), I.pause = c.pause.bind(c), I.resume = c.resume.bind(c), I.stop = I, I;
}
function X(e, t = 1 / 0, n) {
  if (t <= 0 || !T(e) || e.__v_skip || (n = n || /* @__PURE__ */ new Map(), (n.get(e) || 0) >= t))
    return e;
  if (n.set(e, t), t--, /* @__PURE__ */ R(e))
    X(e.value, t, n);
  else if (O(e))
    for (let s = 0; s < e.length; s++)
      X(e[s], t, n);
  else if (zt(e) || oe(e))
    e.forEach((s) => {
      X(s, t, n);
    });
  else if (Ft(e)) {
    for (const s in e)
      X(e[s], t, n);
    for (const s of Object.getOwnPropertySymbols(e))
      Object.prototype.propertyIsEnumerable.call(e, s) && X(e[s], t, n);
  }
  return e;
}
/**
* @vue/runtime-core v3.5.33
* (c) 2018-present Yuxi (Evan) You and Vue contributors
* @license MIT
**/
const le = [];
function mr(e) {
  le.push(e);
}
function vr() {
  le.pop();
}
let st = !1;
function C(e, ...t) {
  if (st) return;
  st = !0, ye();
  const n = le.length ? le[le.length - 1].component : null, s = n && n.appContext.config.warnHandler, r = yr();
  if (s)
    Xe(
      s,
      n,
      11,
      [
        // eslint-disable-next-line no-restricted-syntax
        e + t.map((o) => {
          var a, i;
          return (i = (a = o.toString) == null ? void 0 : a.call(o)) != null ? i : JSON.stringify(o);
        }).join(""),
        n && n.proxy,
        r.map(
          ({ vnode: o }) => `at <${Sn(n, o.type)}>`
        ).join(`
`),
        r
      ]
    );
  else {
    const o = [`[Vue warn]: ${e}`, ...t];
    r.length && o.push(`
`, ...br(r)), console.warn(...o);
  }
  be(), st = !1;
}
function yr() {
  let e = le[le.length - 1];
  if (!e)
    return [];
  const t = [];
  for (; e; ) {
    const n = t[0];
    n && n.vnode === e ? n.recurseCount++ : t.push({
      vnode: e,
      recurseCount: 0
    });
    const s = e.component && e.component.parent;
    e = s && s.vnode;
  }
  return t;
}
function br(e) {
  const t = [];
  return e.forEach((n, s) => {
    t.push(...s === 0 ? [] : [`
`], ..._r(n));
  }), t;
}
function _r({ vnode: e, recurseCount: t }) {
  const n = t > 0 ? `... (${t} recursive calls)` : "", s = e.component ? e.component.parent == null : !1, r = ` at <${Sn(
    e.component,
    e.type,
    s
  )}`, o = ">" + n;
  return e.props ? [r, ...wr(e.props), o] : [r + o];
}
function wr(e) {
  const t = [], n = Object.keys(e);
  return n.slice(0, 3).forEach((s) => {
    t.push(...an(s, e[s]));
  }), n.length > 3 && t.push(" ..."), t;
}
function an(e, t, n) {
  return H(t) ? (t = JSON.stringify(t), n ? t : [`${e}=${t}`]) : typeof t == "number" || typeof t == "boolean" || t == null ? n ? t : [`${e}=${t}`] : /* @__PURE__ */ R(t) ? (t = an(e, /* @__PURE__ */ m(t.value), !0), n ? t : [`${e}=Ref<`, t, ">"]) : D(t) ? [`${e}=fn${t.name ? `<${t.name}>` : ""}`] : (t = /* @__PURE__ */ m(t), n ? t : [`${e}=`, t]);
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
function Xe(e, t, n, s) {
  try {
    return s ? e(...s) : e();
  } catch (r) {
    Ot(r, t, n);
  }
}
function Nt(e, t, n, s) {
  if (D(e)) {
    const r = Xe(e, t, n, s);
    return r && Pn(r) && r.catch((o) => {
      Ot(o, t, n);
    }), r;
  }
  if (O(e)) {
    const r = [];
    for (let o = 0; o < e.length; o++)
      r.push(Nt(e[o], t, n, s));
    return r;
  } else process.env.NODE_ENV !== "production" && C(
    `Invalid value type passed to callWithAsyncErrorHandling(): ${typeof e}`
  );
}
function Ot(e, t, n, s = !0) {
  const r = t ? t.vnode : null, { errorHandler: o, throwUnhandledErrorInProduction: a } = t && t.appContext.config || Oe;
  if (t) {
    let i = t.parent;
    const l = t.proxy, u = process.env.NODE_ENV !== "production" ? St[n] : `https://vuejs.org/error-reference/#runtime-${n}`;
    for (; i; ) {
      const d = i.ec;
      if (d) {
        for (let c = 0; c < d.length; c++)
          if (d[c](e, l, u) === !1)
            return;
      }
      i = i.parent;
    }
    if (o) {
      ye(), Xe(o, null, 10, [
        e,
        l,
        u
      ]), be();
      return;
    }
  }
  xr(e, n, r, s, a);
}
function xr(e, t, n, s = !0, r = !1) {
  if (process.env.NODE_ENV !== "production") {
    const o = St[t];
    if (n && mr(n), C(`Unhandled error${o ? ` during execution of ${o}` : ""}`), n && vr(), s)
      throw e;
    console.error(e);
  } else {
    if (r)
      throw e;
    console.error(e);
  }
}
const P = [];
let q = -1;
const ge = [];
let Q = null, fe = 0;
const ln = /* @__PURE__ */ Promise.resolve();
let We = null;
const kr = 100;
function cn(e) {
  const t = We || ln;
  return e ? t.then(this ? e.bind(this) : e) : t;
}
function Er(e) {
  let t = q + 1, n = P.length;
  for (; t < n; ) {
    const s = t + n >>> 1, r = P[s], o = Ie(r);
    o < e || o === e && r.flags & 2 ? t = s + 1 : n = s;
  }
  return t;
}
function un(e) {
  if (!(e.flags & 1)) {
    const t = Ie(e), n = P[P.length - 1];
    !n || // fast path when the job id is larger than the tail
    !(e.flags & 2) && t >= Ie(n) ? P.push(e) : P.splice(Er(t), 0, e), e.flags |= 1, dn();
  }
}
function dn() {
  We || (We = ln.then(pn));
}
function fn(e) {
  O(e) ? ge.push(...e) : Q && e.id === -1 ? Q.splice(fe + 1, 0, e) : e.flags & 1 || (ge.push(e), e.flags |= 1), dn();
}
function Sr(e) {
  if (ge.length) {
    const t = [...new Set(ge)].sort(
      (n, s) => Ie(n) - Ie(s)
    );
    if (ge.length = 0, Q) {
      Q.push(...t);
      return;
    }
    for (Q = t, process.env.NODE_ENV !== "production" && (e = e || /* @__PURE__ */ new Map()), fe = 0; fe < Q.length; fe++) {
      const n = Q[fe];
      process.env.NODE_ENV !== "production" && hn(e, n) || (n.flags & 4 && (n.flags &= -2), n.flags & 8 || n(), n.flags &= -2);
    }
    Q = null, fe = 0;
  }
}
const Ie = (e) => e.id == null ? e.flags & 2 ? -1 : 1 / 0 : e.id;
function pn(e) {
  process.env.NODE_ENV !== "production" && (e = e || /* @__PURE__ */ new Map());
  const t = process.env.NODE_ENV !== "production" ? (n) => hn(e, n) : he;
  try {
    for (q = 0; q < P.length; q++) {
      const n = P[q];
      if (n && !(n.flags & 8)) {
        if (process.env.NODE_ENV !== "production" && t(n))
          continue;
        n.flags & 4 && (n.flags &= -2), Xe(
          n,
          n.i,
          n.i ? 15 : 14
        ), n.flags & 4 || (n.flags &= -2);
      }
    }
  } finally {
    for (; q < P.length; q++) {
      const n = P[q];
      n && (n.flags &= -2);
    }
    q = -1, P.length = 0, Sr(e), We = null, (P.length || ge.length) && pn(e);
  }
}
function hn(e, t) {
  const n = e.get(t) || 0;
  if (n > kr) {
    const s = t.i, r = s && En(s.type);
    return Ot(
      `Maximum recursive updates exceeded${r ? ` in component <${r}>` : ""}. This means you have a reactive effect that is mutating its own dependencies and thus recursively triggering itself. Possible sources include component template, render function, updated hook or watcher source function.`,
      null,
      10
    ), !0;
  }
  return e.set(t, n + 1), !1;
}
const ot = /* @__PURE__ */ new Map();
process.env.NODE_ENV !== "production" && (Qe().__VUE_HMR_RUNTIME__ = {
  createRecord: it(Nr),
  rerender: it(Or),
  reload: it(Cr)
});
const Be = /* @__PURE__ */ new Map();
function Nr(e, t) {
  return Be.has(e) ? !1 : (Be.set(e, {
    initialDef: Je(t),
    instances: /* @__PURE__ */ new Set()
  }), !0);
}
function Je(e) {
  return Nn(e) ? e.__vccOpts : e;
}
function Or(e, t) {
  const n = Be.get(e);
  n && (n.initialDef.render = t, [...n.instances].forEach((s) => {
    t && (s.render = t, Je(s.type).render = t), s.renderCache = [], s.job.flags & 8 || s.update();
  }));
}
function Cr(e, t) {
  const n = Be.get(e);
  if (!n) return;
  t = Je(t), At(n.initialDef, t);
  const s = [...n.instances];
  for (let r = 0; r < s.length; r++) {
    const o = s[r], a = Je(o.type);
    let i = ot.get(a);
    i || (a !== n.initialDef && At(a, t), ot.set(a, i = /* @__PURE__ */ new Set())), i.add(o), o.appContext.propsCache.delete(o.type), o.appContext.emitsCache.delete(o.type), o.appContext.optionsCache.delete(o.type), o.ceReload ? (i.add(o), o.ceReload(t.styles), i.delete(o)) : o.parent ? un(() => {
      o.job.flags & 8 || (o.parent.update(), i.delete(o));
    }) : o.appContext.reload ? o.appContext.reload() : typeof window < "u" ? window.location.reload() : console.warn(
      "[HMR] Root or manually mounted instance modified. Full reload required."
    ), o.root.ce && o !== o.root && o.root.ce._removeChildStyle(a);
  }
  fn(() => {
    ot.clear();
  });
}
function At(e, t) {
  B(e, t);
  for (const n in e)
    n !== "__file" && !(n in t) && delete e[n];
}
function it(e) {
  return (t, n) => {
    try {
      return e(t, n);
    } catch (s) {
      console.error(s), console.warn(
        "[HMR] Something went wrong during Vue component hot-reload. Full reload required."
      );
    }
  };
}
let pe, Ke = [];
function gn(e, t) {
  var n, s;
  pe = e, pe ? (pe.enabled = !0, Ke.forEach(({ event: r, args: o }) => pe.emit(r, ...o)), Ke = []) : /* handle late devtools injection - only do this if we are in an actual */ /* browser environment to avoid the timer handle stalling test runner exit */ /* (#4815) */ typeof window < "u" && // some envs mock window but not fully
  window.HTMLElement && // also exclude jsdom
  // eslint-disable-next-line no-restricted-syntax
  !((s = (n = window.navigator) == null ? void 0 : n.userAgent) != null && s.includes("jsdom")) ? ((t.__VUE_DEVTOOLS_HOOK_REPLAY__ = t.__VUE_DEVTOOLS_HOOK_REPLAY__ || []).push((o) => {
    gn(o, t);
  }), setTimeout(() => {
    pe || (t.__VUE_DEVTOOLS_HOOK_REPLAY__ = null, Ke = []);
  }, 3e3)) : Ke = [];
}
let Te = null, Dr = null;
function ke(e, t) {
  return process.env.NODE_ENV !== "production" && C("withDirectives can only be used inside render functions."), e;
}
function Ir(e, t, n = !1) {
  const s = kn();
  if (s || Hr) {
    let r = s ? s.parent == null || s.ce ? s.vnode.appContext && s.vnode.appContext.provides : s.parent.provides : void 0;
    if (r && e in r)
      return r[e];
    if (arguments.length > 1)
      return n && D(t) ? t.call(s && s.proxy) : t;
    process.env.NODE_ENV !== "production" && C(`injection "${String(e)}" not found.`);
  } else process.env.NODE_ENV !== "production" && C("inject() can only be used inside setup() or functional components.");
}
const Tr = /* @__PURE__ */ Symbol.for("v-scx"), $r = () => {
  {
    const e = Ir(Tr);
    return e || process.env.NODE_ENV !== "production" && C(
      "Server rendering context not provided. Make sure to only call useSSRContext() conditionally in the server build."
    ), e;
  }
};
function Ar(e, t, n) {
  return process.env.NODE_ENV !== "production" && !D(t) && C(
    "`watch(fn, options?)` signature has been moved to a separate API. Use `watchEffect(fn, options?)` instead. `watch` now only supports `watch(source, cb, options?) signature."
  ), Rr(e, t, n);
}
function Rr(e, t, n = Oe) {
  const { immediate: s, deep: r, flush: o, once: a } = n;
  process.env.NODE_ENV !== "production" && !t && (s !== void 0 && C(
    'watch() "immediate" option is only respected when using the watch(source, callback, options?) signature.'
  ), r !== void 0 && C(
    'watch() "deep" option is only respected when using the watch(source, callback, options?) signature.'
  ), a !== void 0 && C(
    'watch() "once" option is only respected when using the watch(source, callback, options?) signature.'
  ));
  const i = B({}, n);
  process.env.NODE_ENV !== "production" && (i.onWarn = C);
  const l = t && s || !t && o !== "post";
  let u;
  if (Ae) {
    if (o === "sync") {
      const g = $r();
      u = g.__watcherHandles || (g.__watcherHandles = []);
    } else if (!l) {
      const g = () => {
      };
      return g.stop = he, g.resume = he, g.pause = he, g;
    }
  }
  const d = _e;
  i.call = (g, y, h) => Nt(g, d, y, h);
  let c = !1;
  o === "post" ? i.scheduler = (g) => {
    Ur(g, d && d.suspense);
  } : o !== "sync" && (c = !0, i.scheduler = (g, y) => {
    y ? g() : un(g);
  }), i.augmentJob = (g) => {
    t && (g.flags |= 4), c && (g.flags |= 2, d && (g.id = d.uid, g.i = d));
  };
  const f = gr(e, t, i);
  return Ae && (u ? u.push(f) : l && f()), f;
}
const Vr = (e) => e.__isTeleport;
function mn(e, t) {
  e.shapeFlag & 6 && e.component ? (e.transition = t, mn(e.component.subTree, t)) : e.shapeFlag & 128 ? (e.ssContent.transition = t.clone(e.ssContent), e.ssFallback.transition = t.clone(e.ssFallback)) : e.transition = t;
}
// @__NO_SIDE_EFFECTS__
function Ve(e, t) {
  return D(e) ? (
    // #8236: extend call and options.name access are considered side-effects
    // by Rollup, so we have to wrap it in a pure-annotated IIFE.
    B({ name: e.name }, t, { setup: e })
  ) : e;
}
Qe().requestIdleCallback;
Qe().cancelIdleCallback;
function Pr(e, t, n = _e, s = !1) {
  if (n) {
    const r = n[e] || (n[e] = []), o = t.__weh || (t.__weh = (...a) => {
      ye();
      const i = Zr(n), l = Nt(t, n, e, a);
      return i(), be(), l;
    });
    return s ? r.unshift(o) : r.push(o), o;
  } else if (process.env.NODE_ENV !== "production") {
    const r = zn(St[e].replace(/ hook$/, ""));
    C(
      `${r} is called when there is no active component instance to be associated with. Lifecycle injection APIs can only be used during execution of setup(). If you are using async setup(), make sure to register lifecycle hooks before the first await statement.`
    );
  }
}
const Mr = (e) => (t, n = _e) => {
  (!Ae || e === "sp") && Pr(e, (...s) => t(...s), n);
}, jr = Mr("m"), zr = /* @__PURE__ */ Symbol.for("v-ndc");
function Ye(e, t, n, s) {
  let r;
  const o = n, a = O(e);
  if (a || H(e)) {
    const i = a && /* @__PURE__ */ ae(e);
    let l = !1, u = !1;
    i && (l = !/* @__PURE__ */ $(e), u = /* @__PURE__ */ F(e), e = Ze(e)), r = new Array(e.length);
    for (let d = 0, c = e.length; d < c; d++)
      r[d] = t(
        l ? u ? ve(U(e[d])) : U(e[d]) : e[d],
        d,
        void 0,
        o
      );
  } else if (typeof e == "number")
    if (process.env.NODE_ENV !== "production" && (!Number.isInteger(e) || e < 0))
      C(
        `The v-for range expects a positive integer value but got ${e}.`
      ), r = [];
    else {
      r = new Array(e);
      for (let i = 0; i < e; i++)
        r[i] = t(i + 1, i, void 0, o);
    }
  else if (T(e))
    if (e[Symbol.iterator])
      r = Array.from(
        e,
        (i, l) => t(i, l, void 0, o)
      );
    else {
      const i = Object.keys(e);
      r = new Array(i.length);
      for (let l = 0, u = i.length; l < u; l++) {
        const d = i[l];
        r[l] = t(e[d], d, l, o);
      }
    }
  else
    r = [];
  return r;
}
const Kr = {};
process.env.NODE_ENV !== "production" && (Kr.ownKeys = (e) => (C(
  "Avoid app logic that relies on enumerating keys on a component instance. The keys will be empty in production mode to avoid performance overhead."
), Reflect.ownKeys(e)));
let Hr = null;
const Fr = {}, vn = (e) => Object.getPrototypeOf(e) === Fr, Ur = Wr, Lr = (e) => e.__isSuspense;
function Wr(e, t) {
  t && t.pendingBranch ? O(e) ? t.effects.push(...e) : t.effects.push(e) : fn(e);
}
const ee = /* @__PURE__ */ Symbol.for("v-fgt"), Br = /* @__PURE__ */ Symbol.for("v-txt"), ht = /* @__PURE__ */ Symbol.for("v-cmt"), He = [];
let z = null;
function w(e = !1) {
  He.push(z = e ? null : []);
}
function Jr() {
  He.pop(), z = He[He.length - 1] || null;
}
function yn(e) {
  return e.dynamicChildren = z || $n, Jr(), z && z.push(e), e;
}
function E(e, t, n, s, r, o) {
  return yn(
    p(
      e,
      t,
      n,
      s,
      r,
      o,
      !0
    )
  );
}
function gt(e, t, n, s, r) {
  return yn(
    $e(
      e,
      t,
      n,
      s,
      r,
      !0
    )
  );
}
function Yr(e) {
  return e ? e.__v_isVNode === !0 : !1;
}
const qr = (...e) => _n(
  ...e
), bn = ({ key: e }) => e ?? null, Fe = ({
  ref: e,
  ref_key: t,
  ref_for: n
}) => (typeof e == "number" && (e = "" + e), e != null ? H(e) || /* @__PURE__ */ R(e) || D(e) ? { i: Te, r: e, k: t, f: !!n } : e : null);
function p(e, t = null, n = null, s = 0, r = null, o = e === ee ? 0 : 1, a = !1, i = !1) {
  const l = {
    __v_isVNode: !0,
    __v_skip: !0,
    type: e,
    props: t,
    key: t && bn(t),
    ref: t && Fe(t),
    scopeId: Dr,
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
    patchFlag: s,
    dynamicProps: r,
    dynamicChildren: null,
    appContext: null,
    ctx: Te
  };
  return i ? (Ct(l, n), o & 128 && e.normalize(l)) : n && (l.shapeFlag |= H(n) ? 8 : 16), process.env.NODE_ENV !== "production" && l.key !== l.key && C("VNode created with invalid key (NaN). VNode type:", l.type), // avoid a block node from tracking itself
  !a && // has current parent block
  z && // presence of a patch flag indicates this node needs patching on updates.
  // component nodes also should always be patched, because even if the
  // component doesn't need to update, it needs to persist the instance on to
  // the next vnode so that it can be properly unmounted later.
  (l.patchFlag > 0 || o & 6) && // the EVENTS flag is only for hydration and if it is the only flag, the
  // vnode should not be considered dynamic due to handler caching.
  l.patchFlag !== 32 && z.push(l), l;
}
const $e = process.env.NODE_ENV !== "production" ? qr : _n;
function _n(e, t = null, n = null, s = 0, r = null, o = !1) {
  if ((!e || e === zr) && (process.env.NODE_ENV !== "production" && !e && C(`Invalid vnode type when creating vnode: ${e}.`), e = ht), Yr(e)) {
    const i = qe(
      e,
      t,
      !0
      /* mergeRef: true */
    );
    return n && Ct(i, n), !o && z && (i.shapeFlag & 6 ? z[z.indexOf(e)] = i : z.push(i)), i.patchFlag = -2, i;
  }
  if (Nn(e) && (e = e.__vccOpts), t) {
    t = Gr(t);
    let { class: i, style: l } = t;
    i && !H(i) && (t.class = me(i)), T(l) && (/* @__PURE__ */ Ue(l) && !O(l) && (l = B({}, l)), t.style = bt(l));
  }
  const a = H(e) ? 1 : Lr(e) ? 128 : Vr(e) ? 64 : T(e) ? 4 : D(e) ? 2 : 0;
  return process.env.NODE_ENV !== "production" && a & 4 && /* @__PURE__ */ Ue(e) && (e = /* @__PURE__ */ m(e), C(
    "Vue received a Component that was made a reactive object. This can lead to unnecessary performance overhead and should be avoided by marking the component with `markRaw` or using `shallowRef` instead of `ref`.",
    `
Component that was made reactive: `,
    e
  )), p(
    e,
    t,
    n,
    s,
    r,
    a,
    o,
    !0
  );
}
function Gr(e) {
  return e ? /* @__PURE__ */ Ue(e) || vn(e) ? B({}, e) : e : null;
}
function qe(e, t, n = !1, s = !1) {
  const { props: r, ref: o, patchFlag: a, children: i, transition: l } = e, u = t ? Qr(r || {}, t) : r, d = {
    __v_isVNode: !0,
    __v_skip: !0,
    type: e.type,
    props: u,
    key: u && bn(u),
    ref: t && t.ref ? (
      // #2078 in the case of <component :is="vnode" ref="extra"/>
      // if the vnode itself already has a ref, cloneVNode will need to merge
      // the refs so the single vnode can be set on multiple refs
      n && o ? O(o) ? o.concat(Fe(t)) : [o, Fe(t)] : Fe(t)
    ) : o,
    scopeId: e.scopeId,
    slotScopeIds: e.slotScopeIds,
    children: process.env.NODE_ENV !== "production" && a === -1 && O(i) ? i.map(wn) : i,
    target: e.target,
    targetStart: e.targetStart,
    targetAnchor: e.targetAnchor,
    staticCount: e.staticCount,
    shapeFlag: e.shapeFlag,
    // if the vnode is cloned with extra props, we can no longer assume its
    // existing patch flag to be reliable and need to add the FULL_PROPS flag.
    // note: preserve flag for fragments since they use the flag for children
    // fast paths only.
    patchFlag: t && e.type !== ee ? a === -1 ? 16 : a | 16 : a,
    dynamicProps: e.dynamicProps,
    dynamicChildren: e.dynamicChildren,
    appContext: e.appContext,
    dirs: e.dirs,
    transition: l,
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
  return l && s && mn(
    d,
    l.clone(d)
  ), d;
}
function wn(e) {
  const t = qe(e);
  return O(e.children) && (t.children = e.children.map(wn)), t;
}
function xn(e = " ", t = 0) {
  return $e(Br, null, e, t);
}
function te(e = "", t = !1) {
  return t ? (w(), gt(ht, null, e)) : $e(ht, null, e);
}
function Ct(e, t) {
  let n = 0;
  const { shapeFlag: s } = e;
  if (t == null)
    t = null;
  else if (O(t))
    n = 16;
  else if (typeof t == "object")
    if (s & 65) {
      const r = t.default;
      r && (r._c && (r._d = !1), Ct(e, r()), r._c && (r._d = !0));
      return;
    } else
      n = 32, !t._ && !vn(t) && (t._ctx = Te);
  else D(t) ? (t = { default: t, _ctx: Te }, n = 32) : (t = String(t), s & 64 ? (n = 16, t = [xn(t)]) : n = 8);
  e.children = t, e.shapeFlag |= n;
}
function Qr(...e) {
  const t = {};
  for (let n = 0; n < e.length; n++) {
    const s = e[n];
    for (const r in s)
      if (r === "class")
        t.class !== s.class && (t.class = me([t.class, s.class]));
      else if (r === "style")
        t.style = bt([t.style, s.style]);
      else if (An(r)) {
        const o = t[r], a = s[r];
        a && o !== a && !(O(o) && o.includes(a)) ? t[r] = o ? [].concat(o, a) : a : a == null && o == null && // mergeProps({ 'onUpdate:modelValue': undefined }) should not retain
        // the model listener.
        !Rn(r) && (t[r] = a);
      } else r !== "" && (t[r] = s[r]);
  }
  return t;
}
let _e = null;
const kn = () => _e || Te;
let mt;
{
  const e = Qe(), t = (n, s) => {
    let r;
    return (r = e[n]) || (r = e[n] = []), r.push(s), (o) => {
      r.length > 1 ? r.forEach((a) => a(o)) : r[0](o);
    };
  };
  mt = t(
    "__VUE_INSTANCE_SETTERS__",
    (n) => _e = n
  ), t(
    "__VUE_SSR_SETTERS__",
    (n) => Ae = n
  );
}
const Zr = (e) => {
  const t = _e;
  return mt(e), e.scope.on(), () => {
    e.scope.off(), mt(t);
  };
};
let Ae = !1;
process.env.NODE_ENV;
const Xr = /(?:^|[-_])\w/g, es = (e) => e.replace(Xr, (t) => t.toUpperCase()).replace(/[-_]/g, "");
function En(e, t = !0) {
  return D(e) ? e.displayName || e.name : e.name || t && e.__name;
}
function Sn(e, t, n = !1) {
  let s = En(t);
  if (!s && t.__file) {
    const r = t.__file.match(/([^/\\]+)\.\w+$/);
    r && (s = r[1]);
  }
  if (!s && e) {
    const r = (o) => {
      for (const a in o)
        if (o[a] === t)
          return a;
    };
    s = r(e.components) || e.parent && r(
      e.parent.type.components
    ) || r(e.appContext.components);
  }
  return s ? es(s) : n ? "App" : "Anonymous";
}
function Nn(e) {
  return D(e) && "__vccOpts" in e;
}
const Re = (e, t) => {
  const n = /* @__PURE__ */ pr(e, t, Ae);
  if (process.env.NODE_ENV !== "production") {
    const s = kn();
    s && s.appContext.config.warnRecursiveComputed && (n._warnRecursive = !0);
  }
  return n;
};
function ts() {
  if (process.env.NODE_ENV === "production" || typeof window > "u")
    return;
  const e = { style: "color:#3ba776" }, t = { style: "color:#1677ff" }, n = { style: "color:#f5222d" }, s = { style: "color:#eb2f96" }, r = {
    __vue_custom_formatter: !0,
    header(c) {
      if (!T(c))
        return null;
      if (c.__isVue)
        return ["div", e, "VueInstance"];
      if (/* @__PURE__ */ R(c)) {
        ye();
        const f = c.value;
        return be(), [
          "div",
          {},
          ["span", e, d(c)],
          "<",
          i(f),
          ">"
        ];
      } else {
        if (/* @__PURE__ */ ae(c))
          return [
            "div",
            {},
            ["span", e, /* @__PURE__ */ $(c) ? "ShallowReactive" : "Reactive"],
            "<",
            i(c),
            `>${/* @__PURE__ */ F(c) ? " (readonly)" : ""}`
          ];
        if (/* @__PURE__ */ F(c))
          return [
            "div",
            {},
            ["span", e, /* @__PURE__ */ $(c) ? "ShallowReadonly" : "Readonly"],
            "<",
            i(c),
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
    c.type.props && c.props && f.push(a("props", /* @__PURE__ */ m(c.props))), c.setupState !== Oe && f.push(a("setup", c.setupState)), c.data !== Oe && f.push(a("data", /* @__PURE__ */ m(c.data)));
    const g = l(c, "computed");
    g && f.push(a("computed", g));
    const y = l(c, "inject");
    return y && f.push(a("injected", y)), f.push([
      "div",
      {},
      [
        "span",
        {
          style: s.style + ";opacity:0.66"
        },
        "$ (internal): "
      ],
      ["object", { object: c }]
    ]), f;
  }
  function a(c, f) {
    return f = B({}, f), Object.keys(f).length ? [
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
          ["span", s, g + ": "],
          i(f[g], !1)
        ])
      ]
    ] : ["span", {}];
  }
  function i(c, f = !0) {
    return typeof c == "number" ? ["span", t, c] : typeof c == "string" ? ["span", n, JSON.stringify(c)] : typeof c == "boolean" ? ["span", s, c] : T(c) ? ["object", { object: f ? /* @__PURE__ */ m(c) : c }] : ["span", n, String(c)];
  }
  function l(c, f) {
    const g = c.type;
    if (D(g))
      return;
    const y = {};
    for (const h in c.ctx)
      u(g, h, f) && (y[h] = c.ctx[h]);
    return y;
  }
  function u(c, f, g) {
    const y = c[g];
    if (O(y) && y.includes(f) || T(y) && f in y || c.extends && u(c.extends, f, g) || c.mixins && c.mixins.some((h) => u(h, f, g)))
      return !0;
  }
  function d(c) {
    return /* @__PURE__ */ $(c) ? "ShallowRef" : c.effect ? "ComputedRef" : "Ref";
  }
  window.devtoolsFormatters ? window.devtoolsFormatters.push(r) : window.devtoolsFormatters = [r];
}
const ns = process.env.NODE_ENV !== "production" ? C : he;
process.env.NODE_ENV;
process.env.NODE_ENV;
/**
* @vue/runtime-dom v3.5.33
* (c) 2018-present Yuxi (Evan) You and Vue contributors
* @license MIT
**/
let rs;
const Rt = typeof window < "u" && window.trustedTypes;
if (Rt)
  try {
    rs = /* @__PURE__ */ Rt.createPolicy("vue", {
      createHTML: (e) => e
    });
  } catch (e) {
    process.env.NODE_ENV !== "production" && ns(`Error creating trusted types policy: ${e}`);
  }
process.env.NODE_ENV;
function xe(e, t, n, s) {
  e.addEventListener(t, n, s);
}
const Vt = (e) => {
  const t = e.props["onUpdate:modelValue"] || !1;
  return O(t) ? (n) => Kn(t, n) : t;
};
function ss(e) {
  e.target.composing = !0;
}
function Pt(e) {
  const t = e.target;
  t.composing && (t.composing = !1, t.dispatchEvent(new Event("input")));
}
const at = /* @__PURE__ */ Symbol("_assign");
function Mt(e, t, n) {
  return t && (e = e.trim()), n && (e = Lt(e)), e;
}
const Ee = {
  created(e, { modifiers: { lazy: t, trim: n, number: s } }, r) {
    e[at] = Vt(r);
    const o = s || r.props && r.props.type === "number";
    xe(e, t ? "change" : "input", (a) => {
      a.target.composing || e[at](Mt(e.value, n, o));
    }), (n || o) && xe(e, "change", () => {
      e.value = Mt(e.value, n, o);
    }), t || (xe(e, "compositionstart", ss), xe(e, "compositionend", Pt), xe(e, "change", Pt));
  },
  // set value on mounted so it's after min/max for type="range"
  mounted(e, { value: t }) {
    e.value = t ?? "";
  },
  beforeUpdate(e, { value: t, oldValue: n, modifiers: { lazy: s, trim: r, number: o } }, a) {
    if (e[at] = Vt(a), e.composing) return;
    const i = (o || e.type === "number") && !/^0\d/.test(e.value) ? Lt(e.value) : e.value, l = t ?? "";
    if (i === l)
      return;
    const u = e.getRootNode();
    (u instanceof Document || u instanceof ShadowRoot) && u.activeElement === e && e.type !== "range" && (s && t === n || r && e.value.trim() === l) || (e.value = l);
  }
}, os = ["ctrl", "shift", "alt", "meta"], is = {
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
  exact: (e, t) => os.some((n) => e[`${n}Key`] && !t.includes(n))
}, On = (e, t) => {
  if (!e) return e;
  const n = e._withMods || (e._withMods = {}), s = t.join(".");
  return n[s] || (n[s] = (r, ...o) => {
    for (let a = 0; a < t.length; a++) {
      const i = is[t[a]];
      if (i && i(r, t)) return;
    }
    return e(r, ...o);
  });
}, as = {
  esc: "escape",
  space: " ",
  up: "arrow-up",
  left: "arrow-left",
  right: "arrow-right",
  down: "arrow-down",
  delete: "backspace"
}, ls = (e, t) => {
  const n = e._withKeys || (e._withKeys = {}), s = t.join(".");
  return n[s] || (n[s] = (r) => {
    if (!("key" in r))
      return;
    const o = jn(r.key);
    if (t.some(
      (a) => a === o || as[a] === o
    ))
      return e(r);
  });
};
/**
* vue v3.5.33
* (c) 2018-present Yuxi (Evan) You and Vue contributors
* @license MIT
**/
function cs() {
  ts();
}
process.env.NODE_ENV !== "production" && cs();
const us = {
  key: 0,
  class: "w-7 h-7 rounded-full bg-primary-100 dark:bg-primary-900 flex items-center justify-center text-xs flex-shrink-0 mt-1"
}, ds = {
  key: 0,
  class: "flex items-center gap-1"
}, fs = ["innerHTML"], ps = {
  key: 2,
  class: "whitespace-pre-wrap"
}, hs = {
  key: 1,
  class: "w-7 h-7 rounded-full bg-slate-200 dark:bg-dark-600 flex items-center justify-center text-xs flex-shrink-0 mt-1"
}, gs = /* @__PURE__ */ Ve({
  __name: "ChatMessage",
  props: {
    message: {},
    streaming: { type: Boolean }
  },
  setup(e) {
    const t = e, n = Re(() => {
      let s = t.message.content;
      return s = s.replace(/```(\w*)\n([\s\S]*?)```/g, '<pre class="bg-slate-800 text-green-300 rounded p-2 my-1 overflow-x-auto text-xs"><code>$2</code></pre>'), s = s.replace(/`([^`]+)`/g, '<code class="bg-slate-200 dark:bg-dark-600 px-1 rounded text-xs">$1</code>'), s = s.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>"), s = s.replace(/\*([^*]+)\*/g, "<em>$1</em>"), s = s.replace(/\n/g, "<br>"), s;
    });
    return (s, r) => (w(), E("div", {
      class: me(["flex gap-3", e.message.role === "user" ? "justify-end" : "justify-start"])
    }, [
      e.message.role === "assistant" ? (w(), E("div", us, " AI ")) : te("", !0),
      p("div", {
        class: me([
          "max-w-[85%] rounded-lg px-3 py-2 text-sm leading-relaxed",
          e.message.role === "user" ? "bg-primary-600 text-white" : "bg-slate-100 dark:bg-dark-700 text-slate-800 dark:text-dark-200"
        ])
      }, [
        e.message.role === "assistant" && !e.message.content && e.streaming ? (w(), E("div", ds, [...r[0] || (r[0] = [
          p("span", { class: "inline-block w-1.5 h-4 bg-primary-500 animate-pulse" }, null, -1)
        ])])) : e.message.role === "assistant" ? (w(), E("div", {
          key: 1,
          innerHTML: n.value
        }, null, 8, fs)) : (w(), E("div", ps, G(e.message.content), 1))
      ], 2),
      e.message.role === "user" ? (w(), E("div", hs, " 我 ")) : te("", !0)
    ], 2));
  }
}), ms = { class: "flex gap-2 items-end" }, vs = ["placeholder", "disabled", "onKeydown"], ys = ["disabled"], bs = /* @__PURE__ */ Ve({
  __name: "ChatInput",
  props: {
    disabled: { type: Boolean, default: !1 },
    placeholder: { default: "输入消息..." }
  },
  emits: ["send"],
  setup(e, { emit: t }) {
    const n = e, s = t, r = /* @__PURE__ */ N(""), o = /* @__PURE__ */ N(null);
    function a() {
      const l = r.value.trim();
      !l || n.disabled || (s("send", l), r.value = "", cn(() => i()));
    }
    function i() {
      const l = o.value;
      l && (l.style.height = "auto", l.style.height = Math.min(l.scrollHeight, 120) + "px");
    }
    return (l, u) => (w(), E("div", ms, [
      ke(p("textarea", {
        ref_key: "inputRef",
        ref: o,
        "onUpdate:modelValue": u[0] || (u[0] = (d) => r.value = d),
        placeholder: e.placeholder,
        disabled: e.disabled,
        rows: "1",
        class: "flex-1 resize-none bg-slate-50 dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded-lg px-3 py-2 text-sm text-slate-900 dark:text-white placeholder-slate-400 dark:placeholder-dark-500 focus:border-primary-500 outline-none",
        onKeydown: ls(On(a, ["exact", "prevent"]), ["enter"]),
        onInput: i
      }, null, 40, vs), [
        [Ee, r.value]
      ]),
      p("button", {
        disabled: e.disabled || !r.value.trim(),
        class: "px-3 py-2 bg-primary-600 hover:bg-primary-700 disabled:opacity-50 disabled:cursor-not-allowed text-white rounded-lg text-sm font-medium transition-colors flex-shrink-0",
        onClick: a
      }, " 发送 ", 8, ys)
    ]));
  }
});
function jt(e) {
  var s, r, o;
  const t = e.trim();
  if (!t.startsWith("data: ")) return null;
  const n = t.slice(6);
  if (n === "[DONE]") return null;
  try {
    return ((o = (r = (s = JSON.parse(n).choices) == null ? void 0 : s[0]) == null ? void 0 : r.delta) == null ? void 0 : o.content) ?? null;
  } catch {
    return null;
  }
}
async function _s(e, t, n, s) {
  var u, d;
  const r = `${e.baseUrl}/chat/completions`;
  let o;
  try {
    o = await fetch(r, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${e.apiKey}`
      },
      body: JSON.stringify({
        model: e.model,
        messages: t.map((c) => ({ role: c.role, content: c.content })),
        stream: !0
      }),
      signal: s
    });
  } catch (c) {
    c.name !== "AbortError" && n.onError(new Error(`网络请求失败: ${c.message}`));
    return;
  }
  if (!o.ok) {
    let c = `HTTP ${o.status}`;
    try {
      c = ((u = (await o.json()).error) == null ? void 0 : u.message) || c;
    } catch {
    }
    n.onError(new Error(c));
    return;
  }
  const a = (d = o.body) == null ? void 0 : d.getReader();
  if (!a) {
    n.onError(new Error("无法读取响应流"));
    return;
  }
  const i = new TextDecoder();
  let l = "";
  try {
    for (; ; ) {
      const { done: c, value: f } = await a.read();
      if (c) break;
      l += i.decode(f, { stream: !0 });
      const g = l.split(`
`);
      l = g.pop() || "";
      for (const y of g) {
        const h = jt(y);
        h !== null && n.onChunk(h);
      }
    }
    if (l.trim()) {
      const c = jt(l);
      c !== null && n.onChunk(c);
    }
    n.onDone();
  } catch (c) {
    c.name !== "AbortError" && n.onError(new Error(`流读取失败: ${c.message}`));
  }
}
async function ws(e, t, n) {
  var a, i, l, u;
  const s = `${e.baseUrl}/chat/completions`, r = await fetch(s, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${e.apiKey}`
    },
    body: JSON.stringify({
      model: e.model,
      messages: t.map((d) => ({ role: d.role, content: d.content })),
      stream: !1
    }),
    signal: n
  });
  if (!r.ok) {
    let d = `HTTP ${r.status}`;
    try {
      d = ((a = (await r.json()).error) == null ? void 0 : a.message) || d;
    } catch {
    }
    throw new Error(d);
  }
  return ((u = (l = (i = (await r.json()).choices) == null ? void 0 : i[0]) == null ? void 0 : l.message) == null ? void 0 : u.content) || "";
}
const Cn = [
  { name: "DeepSeek", baseUrl: "https://api.deepseek.com/v1", model: "deepseek-chat" },
  { name: "OpenAI", baseUrl: "https://api.openai.com/v1", model: "gpt-4o-mini" },
  { name: "通义千问", baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1", model: "qwen-turbo" },
  { name: "Moonshot", baseUrl: "https://api.moonshot.cn/v1", model: "moonshot-v1-8k" },
  { name: "智谱", baseUrl: "https://open.bigmodel.cn/api/paas/v4", model: "glm-4-flash" },
  { name: "硅基流动", baseUrl: "https://api.siliconflow.cn/v1", model: "Qwen/Qwen2.5-7B-Instruct" }
], xs = { class: "p-4 space-y-3 border-b border-slate-200 dark:border-dark-700 bg-slate-50 dark:bg-dark-800" }, ks = ["onClick"], Es = { class: "text-sm font-medium text-slate-800 dark:text-white" }, Ss = { class: "text-xs text-slate-400 dark:text-dark-500" }, Ns = { class: "flex items-center gap-2" }, Os = {
  key: 0,
  class: "text-xs text-primary-600 dark:text-primary-400"
}, Cs = ["onClick"], Ds = { class: "border border-dashed border-slate-300 dark:border-dark-600 rounded-lg p-3 space-y-2" }, Is = { class: "flex flex-wrap gap-2" }, Ts = ["onClick"], $s = {
  key: 0,
  class: "space-y-2 pt-2"
}, As = { class: "flex gap-2" }, Rs = ["disabled"], Vs = /* @__PURE__ */ Ve({
  __name: "ProviderManager",
  props: {
    providers: {},
    activeProviderName: {}
  },
  emits: ["setActive", "remove", "add"],
  setup(e, { emit: t }) {
    const n = t, s = Cn, r = /* @__PURE__ */ N(null), o = /* @__PURE__ */ Et({ name: "", apiKey: "", baseUrl: "", model: "" });
    function a(l) {
      r.value = l, o.name = l.name, o.apiKey = "", o.baseUrl = l.baseUrl, o.model = l.model;
    }
    function i() {
      !o.name || !o.apiKey || (n("add", { name: o.name, apiKey: o.apiKey, baseUrl: o.baseUrl, model: o.model }), r.value = null, o.name = "", o.apiKey = "", o.baseUrl = "", o.model = "");
    }
    return (l, u) => (w(), E("div", xs, [
      u[6] || (u[6] = p("h4", { class: "text-sm font-semibold text-slate-700 dark:text-dark-300" }, "模型配置", -1)),
      (w(!0), E(ee, null, Ye(e.providers, (d) => (w(), E("div", {
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
            p("div", Es, G(d.name), 1),
            p("div", Ss, G(d.model), 1)
          ]),
          p("div", Ns, [
            d.name === e.activeProviderName ? (w(), E("span", Os, "当前")) : te("", !0),
            p("button", {
              class: "text-xs text-red-500 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300",
              onClick: On((c) => n("remove", d.name), ["stop"])
            }, " 删除 ", 8, Cs)
          ])
        ], 10, ks)
      ]))), 128)),
      p("div", Ds, [
        u[5] || (u[5] = p("h5", { class: "text-xs font-medium text-slate-500 dark:text-dark-400" }, "从预设添加", -1)),
        p("div", Is, [
          (w(!0), E(ee, null, Ye(_(s), (d) => (w(), E("button", {
            key: d.name,
            class: "px-2 py-1 text-xs bg-slate-100 dark:bg-dark-700 text-slate-600 dark:text-dark-300 rounded hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors",
            onClick: (c) => a(d)
          }, G(d.name), 9, Ts))), 128))
        ]),
        r.value ? (w(), E("div", $s, [
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
          p("div", As, [
            p("button", {
              disabled: !o.name || !o.apiKey,
              class: "px-3 py-1.5 text-xs bg-primary-600 hover:bg-primary-700 disabled:opacity-50 text-white rounded transition-colors",
              onClick: i
            }, " 添加 ", 8, Rs),
            p("button", {
              class: "px-3 py-1.5 text-xs bg-slate-100 dark:bg-dark-700 text-slate-600 dark:text-dark-300 rounded hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors",
              onClick: u[4] || (u[4] = (d) => r.value = null)
            }, " 取消 ")
          ])
        ])) : te("", !0)
      ])
    ]));
  }
}), Ps = {
  key: 0,
  class: "fixed inset-0 z-50 flex items-center justify-center p-4"
}, Ms = { class: "relative bg-white dark:bg-dark-800 rounded-xl shadow-2xl border border-slate-200 dark:border-dark-700 w-full max-w-lg" }, js = { class: "p-5 space-y-4" }, zs = {
  key: 0,
  class: "p-3 bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 text-sm rounded-lg"
}, Ks = {
  key: 1,
  class: "flex items-center justify-center py-8"
}, Hs = { class: "p-3 bg-slate-50 dark:bg-dark-700 rounded-lg text-sm text-slate-600 dark:text-dark-300 whitespace-pre-wrap" }, Fs = { class: "p-3 bg-primary-50 dark:bg-primary-900/20 rounded-lg text-sm text-primary-800 dark:text-primary-200 whitespace-pre-wrap border border-primary-200 dark:border-primary-800" }, Us = {
  key: 0,
  class: "px-5 py-3 border-t border-slate-100 dark:border-dark-700 flex justify-end gap-2"
}, Ls = ["disabled"], Ws = {
  key: 1,
  class: "px-5 py-3 border-t border-slate-100 dark:border-dark-700 flex justify-end"
}, Bs = /* @__PURE__ */ Ve({
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
    return (s, r) => e.show ? (w(), E("div", Ps, [
      p("div", {
        class: "absolute inset-0 bg-black/40 backdrop-blur-sm",
        onClick: r[0] || (r[0] = (o) => n("cancel"))
      }),
      p("div", Ms, [
        r[7] || (r[7] = p("div", { class: "px-5 py-3 border-b border-slate-100 dark:border-dark-700" }, [
          p("h3", { class: "text-base font-semibold text-slate-800 dark:text-white" }, "AI 提示词优化")
        ], -1)),
        p("div", js, [
          e.error ? (w(), E("div", zs, G(e.error), 1)) : e.optimizing ? (w(), E("div", Ks, [...r[4] || (r[4] = [
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
              xn(" AI 正在优化提示词... ")
            ], -1)
          ])])) : (w(), E(ee, { key: 2 }, [
            p("div", null, [
              r[5] || (r[5] = p("label", { class: "block text-xs font-medium text-slate-500 dark:text-dark-400 mb-1" }, "原始提示词", -1)),
              p("div", Hs, G(e.original), 1)
            ]),
            p("div", null, [
              r[6] || (r[6] = p("label", { class: "block text-xs font-medium text-slate-500 dark:text-dark-400 mb-1" }, "优化后提示词", -1)),
              p("div", Fs, G(e.optimized), 1)
            ])
          ], 64))
        ]),
        !e.optimizing && !e.error ? (w(), E("div", Us, [
          p("button", {
            class: "px-4 py-2 text-sm bg-slate-100 dark:bg-dark-700 text-slate-700 dark:text-dark-300 rounded-lg hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors",
            onClick: r[1] || (r[1] = (o) => n("cancel"))
          }, "取消"),
          p("button", {
            disabled: !e.optimized,
            class: "px-4 py-2 text-sm bg-primary-600 hover:bg-primary-700 disabled:opacity-50 text-white rounded-lg transition-colors",
            onClick: r[2] || (r[2] = (o) => n("accept"))
          }, "采纳并填入终端", 8, Ls)
        ])) : e.error ? (w(), E("div", Ws, [
          p("button", {
            class: "px-4 py-2 text-sm bg-slate-100 dark:bg-dark-700 text-slate-700 dark:text-dark-300 rounded-lg hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors",
            onClick: r[3] || (r[3] = (o) => n("cancel"))
          }, "关闭")
        ])) : te("", !0)
      ])
    ])) : te("", !0);
  }
});
function Js(e, t) {
  const n = /* @__PURE__ */ N([]), s = /* @__PURE__ */ N(""), r = /* @__PURE__ */ N(!1), o = /* @__PURE__ */ N(!1), a = Re(
    () => n.value.find((h) => h.name === s.value)
  ), i = Re(() => n.value.length > 0);
  async function l() {
    r.value = !0;
    try {
      const h = await e("apiProviders");
      if (h) {
        const I = typeof h == "string" ? JSON.parse(h) : h;
        n.value = Array.isArray(I) ? I : [];
      }
      const k = await e("activeProvider");
      s.value = typeof k == "string" ? k : "", !s.value && n.value.length > 0 && (s.value = n.value[0].name);
    } catch (h) {
      console.error("[AI Chatbox] Failed to load config:", h);
    } finally {
      r.value = !1;
    }
  }
  async function u() {
    try {
      await t("apiProviders", JSON.stringify(n.value)), await t("activeProvider", s.value);
    } catch (h) {
      console.error("[AI Chatbox] Failed to save config:", h);
    }
  }
  async function d(h) {
    if (n.value.some((k) => k.name === h.name))
      throw new Error(`Provider "${h.name}" 已存在`);
    n.value.push(h), s.value || (s.value = h.name), await u();
  }
  async function c(h) {
    var k;
    n.value = n.value.filter((I) => I.name !== h), s.value === h && (s.value = ((k = n.value[0]) == null ? void 0 : k.name) || ""), await u();
  }
  async function f(h, k) {
    const I = n.value.findIndex((M) => M.name === h);
    I !== -1 && (n.value[I] = k, s.value === h && (s.value = k.name), await u());
  }
  async function g(h) {
    n.value.some((k) => k.name === h) && (s.value = h, await u());
  }
  async function y(h, k) {
    await d({
      name: h.name,
      apiKey: k,
      baseUrl: h.baseUrl,
      model: h.model
    });
  }
  return {
    providers: n,
    activeProviderName: s,
    activeProvider: a,
    hasProvider: i,
    loading: r,
    showProviderManager: o,
    loadConfig: l,
    addProvider: d,
    removeProvider: c,
    updateProvider: f,
    setActiveProvider: g,
    addFromPreset: y,
    PROVIDER_PRESETS: Cn
  };
}
function Ys(e, t, n, s) {
  const r = /* @__PURE__ */ N([]), o = /* @__PURE__ */ N(""), a = /* @__PURE__ */ N([]), i = /* @__PURE__ */ N(!1), l = /* @__PURE__ */ N(""), u = /* @__PURE__ */ N(!1), d = Re(
    () => r.value.find((b) => b.id === o.value)
  ), c = Re(() => l.value !== "");
  function f() {
    return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
  }
  async function g() {
    u.value = !0;
    try {
      const b = await e("conversations");
      if (b) {
        const S = typeof b == "string" ? JSON.parse(b) : b;
        r.value = Array.isArray(S) ? S : [];
      }
    } catch (b) {
      console.error("[AI Chatbox] Failed to load conversations:", b);
    } finally {
      u.value = !1;
    }
  }
  async function y(b) {
    try {
      const S = await e(`conv:${b}`);
      if (S) {
        const Pe = typeof S == "string" ? JSON.parse(S) : S;
        a.value = Array.isArray(Pe) ? Pe : [];
      } else
        a.value = [];
    } catch (S) {
      console.error("[AI Chatbox] Failed to load messages:", S), a.value = [];
    }
    o.value = b;
  }
  async function h() {
    try {
      await t("conversations", JSON.stringify(r.value));
    } catch (b) {
      console.error("[AI Chatbox] Failed to save conversations:", b);
    }
  }
  async function k() {
    if (o.value)
      try {
        await t(`conv:${o.value}`, JSON.stringify(a.value));
      } catch (b) {
        console.error("[AI Chatbox] Failed to save messages:", b);
      }
  }
  async function I(b) {
    const S = {
      id: f(),
      title: "新对话",
      createdAt: (/* @__PURE__ */ new Date()).toISOString(),
      updatedAt: (/* @__PURE__ */ new Date()).toISOString(),
      providerName: b
    };
    r.value.unshift(S), await h(), await y(S.id);
  }
  async function M(b) {
    try {
      await n(`conv:${b}`);
    } catch (S) {
      console.error("[AI Chatbox] Failed to delete conversation:", S);
    }
    r.value = r.value.filter((S) => S.id !== b), await h(), o.value === b && (o.value = "", a.value = []);
  }
  async function ne(b) {
    const S = s();
    if (!S) throw new Error("请先配置 AI 模型");
    o.value || await I(S.name);
    const Pe = {
      role: "user",
      content: b,
      timestamp: (/* @__PURE__ */ new Date()).toISOString()
    };
    a.value.push(Pe);
    const ue = r.value.find((j) => j.id === o.value);
    ue && ue.title === "新对话" && (ue.title = b.slice(0, 30) + (b.length > 30 ? "..." : ""), ue.updatedAt = (/* @__PURE__ */ new Date()).toISOString(), await h()), await k(), i.value = !0, l.value = "";
    const Dn = {
      role: "assistant",
      content: "",
      timestamp: (/* @__PURE__ */ new Date()).toISOString()
    };
    a.value.push(Dn);
    const In = a.value.filter((j) => j.content || j.role === "assistant").slice(0, -1).map((j) => ({ role: j.role, content: j.content }));
    await _s(
      S,
      In,
      {
        onChunk: (j) => {
          l.value += j;
          const re = a.value[a.value.length - 1];
          re && re.role === "assistant" && (re.content = l.value);
        },
        onDone: async () => {
          i.value = !1, l.value = "", await k(), ue && (ue.updatedAt = (/* @__PURE__ */ new Date()).toISOString(), await h());
        },
        onError: async (j) => {
          i.value = !1, l.value = "";
          const re = a.value[a.value.length - 1];
          re && re.role === "assistant" && (re.content = `❌ ${j.message}`), await k();
        }
      }
    );
  }
  function v() {
    i.value = !1, l.value = "";
  }
  async function V(b) {
    b !== o.value && await y(b);
  }
  return {
    conversations: r,
    currentConvId: o,
    messages: a,
    sending: i,
    isStreaming: c,
    loadingHistory: u,
    currentConversation: d,
    loadConversations: g,
    newConversation: I,
    deleteConversation: M,
    sendMessage: ne,
    stopGeneration: v,
    switchConversation: V
  };
}
const qs = { class: "h-full flex flex-col bg-white dark:bg-dark-900" }, Gs = { class: "px-4 py-2 flex items-center justify-between border-b border-slate-200 dark:border-dark-700 bg-slate-50 dark:bg-dark-800" }, Qs = { class: "flex items-center gap-2" }, Zs = ["value"], Xs = ["value"], eo = {
  key: 1,
  class: "text-xs text-slate-400"
}, to = { class: "flex items-center gap-1" }, no = ["disabled"], ro = {
  key: 1,
  class: "flex-1 flex flex-col items-center justify-center p-6 text-center"
}, so = {
  key: 0,
  class: "flex flex-col items-center justify-center h-full text-center"
}, oo = { class: "border-t border-slate-200 dark:border-dark-700 p-3" }, io = /* @__PURE__ */ Ve({
  __name: "ChatView",
  setup(e) {
    const t = window.__ai_chatbox_context__, n = Js(t.storage.get, t.storage.set), s = Ys(
      t.storage.get,
      t.storage.set,
      t.storage.delete,
      () => n.activeProvider.value
    ), r = window.__ai_chatbox_optimizer__ || {
      showDialog: /* @__PURE__ */ N(!1),
      optimizing: /* @__PURE__ */ N(!1),
      originalText: /* @__PURE__ */ N(""),
      optimizedText: /* @__PURE__ */ N(""),
      errorMessage: /* @__PURE__ */ N(""),
      acceptOptimized: () => {
      },
      cancelOptimize: () => {
      }
    }, o = /* @__PURE__ */ N(null);
    return Ar(() => s.messages.value.length, () => {
      cn(() => {
        o.value && (o.value.scrollTop = o.value.scrollHeight);
      });
    }), jr(async () => {
      await n.loadConfig(), await s.loadConversations();
    }), (a, i) => (w(), E("div", qs, [
      p("header", Gs, [
        p("div", Qs, [
          _(n).hasProvider.value ? (w(), E("select", {
            key: 0,
            value: _(n).activeProviderName.value,
            class: "bg-white dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1 text-xs text-slate-700 dark:text-white outline-none",
            onChange: i[0] || (i[0] = (l) => _(n).setActiveProvider(l.target.value))
          }, [
            (w(!0), E(ee, null, Ye(_(n).providers.value, (l) => (w(), E("option", {
              key: l.name,
              value: l.name
            }, G(l.name), 9, Xs))), 128))
          ], 40, Zs)) : (w(), E("span", eo, "未配置模型"))
        ]),
        p("div", to, [
          p("button", {
            class: "p-1.5 text-slate-500 dark:text-dark-400 hover:bg-slate-200 dark:hover:bg-dark-700 rounded transition-colors",
            title: "模型配置",
            onClick: i[1] || (i[1] = (l) => _(n).showProviderManager.value = !_(n).showProviderManager.value)
          }, [...i[6] || (i[6] = [
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
            disabled: !_(n).hasProvider.value,
            class: "p-1.5 text-slate-500 dark:text-dark-400 hover:bg-slate-200 dark:hover:bg-dark-700 rounded transition-colors disabled:opacity-50",
            title: "新对话",
            onClick: i[2] || (i[2] = (l) => _(s).newConversation(_(n).activeProviderName.value))
          }, [...i[7] || (i[7] = [
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
          ])], 8, no)
        ])
      ]),
      _(n).showProviderManager.value ? (w(), gt(Vs, {
        key: 0,
        providers: _(n).providers.value,
        "active-provider-name": _(n).activeProviderName.value,
        onSetActive: _(n).setActiveProvider,
        onRemove: _(n).removeProvider,
        onAdd: _(n).addProvider
      }, null, 8, ["providers", "active-provider-name", "onSetActive", "onRemove", "onAdd"])) : te("", !0),
      _(n).hasProvider.value ? (w(), E(ee, { key: 2 }, [
        p("div", {
          ref_key: "messagesContainer",
          ref: o,
          class: "flex-1 overflow-y-auto p-4 space-y-3"
        }, [
          _(s).messages.value.length === 0 ? (w(), E("div", so, [...i[10] || (i[10] = [
            p("div", { class: "text-3xl mb-2" }, "💬", -1),
            p("p", { class: "text-sm text-slate-400 dark:text-dark-500" }, "开始新对话", -1)
          ])])) : te("", !0),
          (w(!0), E(ee, null, Ye(_(s).messages.value, (l, u) => (w(), gt(gs, {
            key: u,
            message: l,
            streaming: _(s).isStreaming.value && u === _(s).messages.value.length - 1
          }, null, 8, ["message", "streaming"]))), 128))
        ], 512),
        p("div", oo, [
          $e(bs, {
            disabled: _(s).sending.value || !_(n).activeProvider.value,
            placeholder: "输入消息...",
            onSend: _(s).sendMessage
          }, null, 8, ["disabled", "onSend"])
        ])
      ], 64)) : (w(), E("div", ro, [
        i[8] || (i[8] = p("div", { class: "text-4xl mb-3" }, "🤖", -1)),
        i[9] || (i[9] = p("p", { class: "text-sm text-slate-500 dark:text-dark-400 mb-3" }, "请先配置 AI 模型", -1)),
        p("button", {
          class: "px-4 py-2 text-sm bg-primary-600 hover:bg-primary-700 text-white rounded-lg transition-colors",
          onClick: i[3] || (i[3] = (l) => _(n).showProviderManager.value = !0)
        }, " 配置模型 ")
      ])),
      $e(Bs, {
        show: _(r).showDialog.value,
        optimizing: _(r).optimizing.value,
        original: _(r).originalText.value,
        optimized: _(r).optimizedText.value,
        error: _(r).errorMessage.value,
        onAccept: i[4] || (i[4] = (l) => _(r).acceptOptimized()),
        onCancel: i[5] || (i[5] = (l) => _(r).cancelOptimize())
      }, null, 8, ["show", "optimizing", "original", "optimized", "error"])
    ]));
  }
}), ao = `你是一个提示词优化专家。请优化以下用户输入的提示词，使其更清晰、更具体、更容易让 AI 理解和执行。
要求：
1. 保持原始意图不变
2. 添加必要的上下文和约束条件
3. 使用更精确的表达方式
4. 只输出优化后的提示词，不要添加任何解释、前缀或引号`;
function lo(e, t, n, s) {
  const r = /* @__PURE__ */ N(!1), o = /* @__PURE__ */ N(!1), a = /* @__PURE__ */ N(""), i = /* @__PURE__ */ N(""), l = /* @__PURE__ */ N("");
  let u = "";
  function d() {
    return new Promise((y) => {
      const h = s("com.bedcode.ai-chatbox", "ai-chatbox:currentInput", (k) => {
        h.dispose(), y(k);
      });
      n("ai-chatbox:getCurrentInput"), setTimeout(() => {
        h.dispose(), y({ sessionId: "", text: "" });
      }, 3e3);
    });
  }
  async function c() {
    const y = await e();
    if (!y) {
      l.value = "请先配置 AI 模型", o.value = !0;
      return;
    }
    const h = await d();
    if (!h.text) {
      l.value = "终端无输入内容", o.value = !0;
      return;
    }
    u = h.sessionId, a.value = h.text, l.value = "", r.value = !0, o.value = !0, i.value = "";
    try {
      const k = await ws(y, [
        { role: "system", content: ao, timestamp: (/* @__PURE__ */ new Date()).toISOString() },
        { role: "user", content: h.text, timestamp: (/* @__PURE__ */ new Date()).toISOString() }
      ]);
      i.value = k;
    } catch (k) {
      l.value = k.message || "优化失败";
    } finally {
      r.value = !1;
    }
  }
  async function f() {
    !u || !i.value || (await t(u, "" + i.value), o.value = !1);
  }
  function g() {
    o.value = !1, a.value = "", i.value = "", l.value = "";
  }
  return {
    optimizing: r,
    showDialog: o,
    originalText: a,
    optimizedText: i,
    errorMessage: l,
    optimizePrompt: c,
    acceptOptimized: f,
    cancelOptimize: g
  };
}
async function co(e) {
  window.__ai_chatbox_context__ = e, e.ui.registerSidebarPanel({
    id: "ai-chatbox.sidebar",
    title: "AI 对话",
    component: io
  });
  const t = lo(
    // getActiveProvider：从 storage 读取当前活跃 provider
    async () => {
      const n = await e.storage.get("apiProviders"), s = await e.storage.get("activeProvider");
      if (!(!n || !s))
        try {
          const r = typeof n == "string" ? JSON.parse(n) : n;
          return (Array.isArray(r) ? r : []).find((a) => a.name === s);
        } catch {
          return;
        }
    },
    // sendInput：代理 context.terminal.sendInput
    (n, s) => e.terminal.sendInput(n, s),
    // eventEmit：使用插件事件系统
    (n, ...s) => e.events.emit(n, ...s),
    // eventOn：使用插件事件系统
    (n, s, r) => e.events.on(s, r)
  );
  window.__ai_chatbox_optimizer__ = t, e.ui.registerTerminalToolbarItem({
    id: "ai-optimize-prompt",
    label: "AI 优化",
    icon: "✨",
    onClick: () => t.optimizePrompt()
  }), console.log("[AI Chatbox] Plugin activated");
}
async function uo() {
  delete window.__ai_chatbox_context__, delete window.__ai_chatbox_optimizer__, console.log("[AI Chatbox] Plugin deactivated");
}
export {
  co as activate,
  uo as deactivate
};

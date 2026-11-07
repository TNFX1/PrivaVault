// UI checks that need no browser: run with `node --test tests/ui/ui.test.js`.
"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const UI = path.join(__dirname, "..", "..", "ui");
const read = (p) => fs.readFileSync(path.join(UI, p), "utf8");
const { strength, newer } = require(path.join(UI, "js", "util.js"));

function loadI18n() {
  const sandbox = { window: {} };
  vm.runInNewContext(read("js/i18n.js"), sandbox);
  return sandbox.window.I18N;
}

test("English and Turkish have exactly the same keys", () => {
  const { en, tr } = loadI18n();
  const missingTr = Object.keys(en).filter((k) => !(k in tr));
  const missingEn = Object.keys(tr).filter((k) => !(k in en));
  assert.deepEqual(missingTr, [], "missing in tr");
  assert.deepEqual(missingEn, [], "missing in en");
});

test("placeholders match between languages", () => {
  const { en, tr } = loadI18n();
  const vars = (s) => (s.match(/\{\w+\}/g) || []).sort().join();
  for (const k of Object.keys(en)) assert.equal(vars(tr[k]), vars(en[k]), k);
});

test("every i18n key used in HTML and JS exists", () => {
  const { en } = loadI18n();
  const html = read("index.html");
  const js = read("js/app.js");
  const used = new Set();
  for (const m of html.matchAll(/data-i18n(?:-placeholder|-aria)?="([^"]+)"/g)) used.add(m[1]);
  for (const m of js.matchAll(/\bt\("([a-z]+\.[A-Za-z_.0-9]+)"/g)) used.add(m[1]);
  const missing = [...used].filter((k) => !(k in en));
  assert.deepEqual(missing, []);
});

test("every backend error code has a message", () => {
  const { en } = loadI18n();
  const rust = fs.readFileSync(path.join(UI, "..", "crates/privavault-core/src/error.rs"), "utf8");
  const codes = [...rust.matchAll(/=> "([a-z_]+)"/g)].map((m) => m[1]);
  assert.ok(codes.length > 10);
  for (const c of [...codes, "busy", "locked"]) assert.ok(`err.${c}` in en, `err.${c}`);
});

test("no innerHTML/outerHTML/insertAdjacentHTML/eval anywhere in the UI (XSS regression)", () => {
  for (const f of ["js/app.js", "js/util.js", "js/i18n.js"]) {
    const src = read(f);
    assert.doesNotMatch(src, /\.(inner|outer)HTML\s*=|insertAdjacentHTML|\beval\(|new Function\(|document\.write/, f);
  }
});

test("UI never sets a style attribute (CSP blocks it); CSSOM only", () => {
  const js = read("js/app.js");
  assert.doesNotMatch(js, /setAttribute\(\s*["']style["']/);
  assert.doesNotMatch(js, /\bstyle:\s*[`"']/, "h() style attribute");
});

test("HTML has no inline scripts, inline handlers or remote resources (CSP)", () => {
  const html = read("index.html");
  assert.doesNotMatch(html, /<script(?![^>]*\bsrc=)[^>]*>/, "inline <script>");
  assert.doesNotMatch(html, /\son[a-z]+=/i, "inline event handler");
  assert.doesNotMatch(html, /\sstyle=/i, "inline style attribute");
  assert.doesNotMatch(html, /(src|href)="(https?:)?\/\//, "remote resource");
  const css = read("css/app.css");
  assert.doesNotMatch(css, /url\((["']?)(https?:)?\/\//, "remote CSS resource");
});

test("password strength", () => {
  assert.equal(strength(""), 0);
  assert.equal(strength("abc"), 1);
  assert.equal(strength("password123"), 1);
  assert.equal(strength("aaaaaaaaaaaaaaaa"), 1);
  assert.ok(strength("Tr7!kq2#Lm") >= 2);
  assert.equal(strength("correct horse battery staple ocean"), 4);
  assert.ok(strength("Gizli kasa: 2026 tatil fotoğrafları!") >= 3);
});

test("version comparison for the opt-in update check", () => {
  assert.equal(newer("2.0.0", "2.0.1"), true);
  assert.equal(newer("2.0.0", "2.0.0"), false);
  assert.equal(newer("2.1.0", "2.0.9"), false);
  assert.equal(newer("2.0", "2.0.1"), true);
  assert.equal(newer("1.4.0", "10.0.0"), true);
});

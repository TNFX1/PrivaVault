// Pure helpers shared by the UI and its Node tests (no DOM, no Tauri).
"use strict";

(function (root) {
  // Rough entropy estimate: character pool × length, with penalties for repetition and common
  // patterns. It is advice only; the only hard rule is a minimum length of 8.
  const COMMON = ["password", "parola", "sifre", "şifre", "123456", "qwerty", "letmein", "admin", "iloveyou", "welcome"];
  function strength(pw) {
    if (!pw) return 0;
    let pool = 0;
    if (/[a-z]/.test(pw)) pool += 26;
    if (/[A-Z]/.test(pw)) pool += 26;
    if (/[0-9]/.test(pw)) pool += 10;
    if (/[^A-Za-z0-9]/.test(pw)) pool += 33;
    const unique = new Set(pw).size;
    let bits = Math.log2(Math.max(pool, 2)) * Math.min(pw.length, unique * 2);
    const lower = pw.toLowerCase();
    if (COMMON.some((c) => lower.includes(c))) bits -= 20;
    if (/^(.)\1+$/.test(pw)) bits = 0;
    if (pw.length < 8) return 1;
    if (bits < 45) return 1;
    if (bits < 60) return 2;
    if (bits < 80) return 3;
    return 4;
  }

  function newer(a, b) {
    const pa = a.split(".").map(Number), pb = b.split(".").map(Number);
    for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
      if ((pb[i] || 0) > (pa[i] || 0)) return true;
      if ((pb[i] || 0) < (pa[i] || 0)) return false;
    }
    return false;
  }

  const api = { strength, newer };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  else root.PVUtil = api;
})(typeof window !== "undefined" ? window : globalThis);

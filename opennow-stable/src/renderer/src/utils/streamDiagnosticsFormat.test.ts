/// <reference types="node" />

import assert from "node:assert/strict";
import test from "node:test";

import { formatServerLocation } from "./streamDiagnosticsFormat";

test("zone id formats like the official client label (Japan (NP-TYO-01))", () => {
  assert.equal(formatServerLocation("NP-TYO-01", ""), "Japan (NP-TYO-01)");
});

test("zone LB hostname with brand token reconstructs the datacenter code", () => {
  assert.equal(
    formatServerLocation("prod", "npa-yes-kul-01.yes.geforcenow.nvidiagrid.net"),
    "Malaysia (NP-KUL-01)",
  );
});

test("cloudmatchbeta zone hostname reconstructs the datacenter code", () => {
  assert.equal(
    formatServerLocation("prod", "np-lax-01.cloudmatchbeta.nvidiagrid.net"),
    "US West (NP-LAX-01)",
  );
});

test("server IP hostnames carry no city code and fall back", () => {
  assert.equal(
    formatServerLocation("prod", "183-78-14-236.yes.geforcenow.nvidiagrid.net"),
    "--",
  );
});

test("city-only hostnames keep the country + city form", () => {
  assert.equal(formatServerLocation("prod", "npa-yes-sin-02.yes.geforcenow.nvidiagrid.net"), "Singapore (NP-SIN-02)");
});

test("empty inputs fall back to --", () => {
  assert.equal(formatServerLocation("", ""), "--");
  assert.equal(formatServerLocation("prod", ""), "--");
});

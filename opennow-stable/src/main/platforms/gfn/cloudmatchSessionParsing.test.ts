/// <reference types="node" />

import assert from "node:assert/strict";
import test from "node:test";

import { firstZoneHostname } from "./cloudmatchSessionParsing";

test("firstZoneHostname prefers the zone LB hostname over a bare server IP", () => {
  assert.equal(
    firstZoneHostname(
      "183-78-14-236.yes.geforcenow.nvidiagrid.net",
      "npa-yes-kul-01.yes.geforcenow.nvidiagrid.net",
    ),
    "npa-yes-kul-01.yes.geforcenow.nvidiagrid.net",
  );
});

test("firstZoneHostname skips IP-shaped candidates entirely", () => {
  assert.equal(firstZoneHostname("203.0.113.10"), undefined);
  assert.equal(firstZoneHostname("183-78-14-236.yes.geforcenow.nvidiagrid.net"), undefined);
});

test("firstZoneHostname accepts a zone hostname in later candidates", () => {
  assert.equal(firstZoneHostname(undefined, "np-lax-01.cloudmatchbeta.nvidiagrid.net"), "np-lax-01.cloudmatchbeta.nvidiagrid.net");
});

test("firstZoneHostname handles array candidates", () => {
  assert.equal(
    firstZoneHostname(["npa-yes-kul-01.yes.geforcenow.nvidiagrid.net"]),
    "npa-yes-kul-01.yes.geforcenow.nvidiagrid.net",
  );
});

test("firstZoneHostname returns undefined when every candidate is empty", () => {
  assert.equal(firstZoneHostname("", undefined, []), undefined);
});

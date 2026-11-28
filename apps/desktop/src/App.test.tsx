import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test } from "vitest";

import { App } from "./App";

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  cleanup();
});

function power() {
  return screen.getByRole("button", { name: /^(ON|OFF)$/ });
}

test("the power button toggles immediately, including rapid clicks", async () => {
  const user = userEvent.setup();
  render(<App />);

  expect(power().textContent).toBe("OFF");
  expect(power().getAttribute("aria-pressed")).toBe("false");
  expect(screen.getByText("Microphone is passed through.")).toBeTruthy();

  await user.click(power());
  expect(power().textContent).toBe("ON");
  expect(power().getAttribute("aria-pressed")).toBe("true");
  expect(screen.getByText("HUSH is cleaning audio.")).toBeTruthy();

  await user.click(power());
  await user.click(power());
  await user.click(power());
  expect(power().textContent).toBe("OFF");
  expect(screen.getByRole("switch", { name: "Clean my microphone" }).getAttribute("aria-checked")).toBe("false");
});

test("the microphone switch and the power button stay on the same setting", async () => {
  const user = userEvent.setup();
  render(<App />);
  const mic = screen.getByRole("switch", { name: "Clean my microphone" });
  expect(mic.getAttribute("aria-checked")).toBe("false");
  await user.click(mic);
  expect(power().textContent).toBe("ON");
  expect(mic.getAttribute("aria-checked")).toBe("true");
  await user.click(power());
  expect(screen.getByRole("switch", { name: "Clean my microphone" }).getAttribute("aria-checked")).toBe("false");
});

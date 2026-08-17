const statusRegion = document.querySelector(".copy-status");
let statusTimer;

function announce(message) {
  if (!(statusRegion instanceof HTMLElement)) return;
  statusRegion.textContent = message;
  statusRegion.classList.add("visible");
  window.clearTimeout(statusTimer);
  statusTimer = window.setTimeout(() => statusRegion.classList.remove("visible"), 1800);
}

async function copyText(value) {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(value);
    return;
  }

  const input = document.createElement("textarea");
  input.value = value;
  input.setAttribute("readonly", "");
  input.style.position = "fixed";
  input.style.opacity = "0";
  document.body.append(input);
  input.select();
  document.execCommand("copy");
  input.remove();
}

for (const button of document.querySelectorAll("[data-copy-target]")) {
  button.addEventListener("click", async () => {
    const targetId = button.getAttribute("data-copy-target");
    const target = targetId ? document.getElementById(targetId) : null;
    const value = target?.textContent?.trim();
    if (!value) return;

    try {
      await copyText(value);
      const previous = button.textContent;
      button.textContent = "Copied";
      announce("Install command copied");
      window.setTimeout(() => {
        button.textContent = previous;
      }, 1600);
    } catch {
      announce("Copy failed. Select the command manually.");
    }
  });
}

for (const tab of document.querySelectorAll("[data-install-tab]")) {
  tab.addEventListener("click", () => {
    const selected = tab.getAttribute("data-install-tab");
    if (!selected) return;
    for (const candidate of document.querySelectorAll("[data-install-tab]")) {
      const active = candidate === tab;
      candidate.classList.toggle("active", active);
      candidate.setAttribute("aria-selected", String(active));
    }
    for (const panel of document.querySelectorAll("[data-install-panel]")) {
      panel.toggleAttribute("hidden", panel.getAttribute("data-install-panel") !== selected);
    }
  });
}

for (const year of document.querySelectorAll("[data-current-year]")) {
  year.textContent = String(new Date().getFullYear());
}

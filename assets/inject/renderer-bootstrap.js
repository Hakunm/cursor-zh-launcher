(() => {
  "use strict";

  const version = __CURSOR_ZH_VERSION__;
  const rules = __CURSOR_ZH_RULES__;
  const collectUntranslated = __CURSOR_ZH_COLLECT__;
  const marker = "__cursorZhRendererVersion";
  if (globalThis[marker] === version) return true;
  globalThis[marker] = version;

  const protectedSelector = [
    ".monaco-editor",
    ".monaco-diff-editor",
    ".terminal",
    ".xterm",
    ".view-lines",
    ".editor-scrollable",
    "textarea",
    "input",
    "pre",
    "code",
    "[contenteditable='true']",
    "[data-message-author-role]",
    "[data-testid='conversation-turn']",
    ".ui-palette-item:has([data-component='glass-palette-agent-workspace-label'])",
    "[data-uri]",
    "[data-path]"
  ].join(",");
  const attributes = ["aria-label", "title", "placeholder"];
  const compositeLabelSelector = ".menubar-menu-title";
  const dynamicStatusSelector = [
    "[id='aiCodeTracking.stats.composer']",
    "[id='aiCodeTracking.stats.tab']"
  ].join(",");
  const dynamicStatus = {
    "aiCodeTracking.stats.composer": {
      textSource: "Agent Stats: {accepted}/{suggested} ({percent}%)",
      textPattern: /^Agent Stats: (\d+)\/(\d+) \((\d+)%\)$/,
      ariaSource: "Agent AI Stats: {accepted} accepted out of {suggested} suggested, {percent} percent acceptance rate",
      ariaPattern: /^Agent AI Stats: (\d+) accepted out of (\d+) suggested, (\d+) percent acceptance rate$/
    },
    "aiCodeTracking.stats.tab": {
      textSource: "Tab Stats: {accepted}/{suggested} ({percent}%)",
      textPattern: /^Tab Stats: (\d+)\/(\d+) \((\d+)%\)$/,
      ariaSource: "Tab AI Stats: {accepted} accepted out of {suggested} suggested, {percent} percent acceptance rate",
      ariaPattern: /^Tab AI Stats: (\d+) accepted out of (\d+) suggested, (\d+) percent acceptance rate$/
    }
  };
  const collectorSelector = [
    "button",
    "[role='menuitem']",
    "[role='option']",
    "[role='dialog'] h1",
    "[role='dialog'] h2",
    "[role='dialog'] h3",
    "[role='dialog'] label",
    ".ui-sidebar-menu-button-label",
    ".ui-sidebar-group-label-title",
    ".ui-pill__label"
  ].join(",");
  const rulesBySource = new Map();
  const untranslated = new Map();

  for (const rule of rules) {
    const existing = rulesBySource.get(rule.source) || [];
    existing.push(rule);
    rulesBySource.set(rule.source, existing);
  }

  function isProtected(element) {
    return !element || !!element.closest?.(protectedSelector);
  }

  function matchingRule(element, source, allowInputPlaceholder = false) {
    const candidates = rulesBySource.get(source);
    if (!candidates) return null;
    if (isProtected(element)) {
      const safeInputPlaceholder = allowInputPlaceholder
        && element?.matches?.("input")
        && !element.parentElement?.closest?.(protectedSelector);
      if (!safeInputPlaceholder) return null;
    }
    return candidates.find((rule) =>
      rule.scopes.some((selector) => element.matches?.(selector) || element.closest?.(selector))
    ) || null;
  }

  function replaceTextNode(node) {
    if (!node || node.nodeType !== Node.TEXT_NODE) return false;
    const parent = node.parentElement;
    const original = node.nodeValue || "";
    const trimmed = original.trim();
    if (!trimmed) return false;
    const rule = matchingRule(parent, trimmed);
    if (!rule) return false;
    const start = original.indexOf(trimmed);
    node.nodeValue = `${original.slice(0, start)}${rule.target}${original.slice(start + trimmed.length)}`;
    return true;
  }

  function replaceCompositeLabel(element) {
    if (!element?.matches?.(compositeLabelSelector)) return false;
    const source = (element.innerText || element.textContent || "").replace(/\s+/g, " ").trim();
    if (!source) return false;
    const rule = matchingRule(element, source);
    if (!rule) return false;
    const mnemonic = element.querySelector("mnemonic");
    if (!mnemonic) {
      element.textContent = rule.target;
      return true;
    }
    const replacement = mnemonic.cloneNode(true);
    element.replaceChildren(
      document.createTextNode(`${rule.target}(`),
      replacement,
      document.createTextNode(")")
    );
    return true;
  }

  function renderDynamicTarget(template, match) {
    const values = {
      accepted: match[1],
      suggested: match[2],
      percent: match[3]
    };
    return template.replace(/\{(accepted|suggested|percent)\}/g, (_, key) => values[key]);
  }

  function replaceDynamicStatus(element) {
    const root = element?.matches?.(dynamicStatusSelector)
      ? element
      : element?.closest?.(dynamicStatusSelector);
    const config = root && dynamicStatus[root.id];
    if (!config) return false;
    const label = root.querySelector(".statusbar-item-label");
    if (!label) return false;
    let changed = false;
    const source = (label.innerText || label.textContent || "").replace(/\s+/g, " ").trim();
    const textMatch = source.match(config.textPattern);
    const textRule = textMatch && matchingRule(root, config.textSource);
    if (textRule) {
      const textNode = [...label.childNodes].find(
        (node) => node.nodeType === Node.TEXT_NODE && node.nodeValue.trim()
      );
      if (textNode) {
        const original = textNode.nodeValue;
        const trimmed = original.trim();
        const start = original.indexOf(trimmed);
        const target = renderDynamicTarget(textRule.target, textMatch);
        textNode.nodeValue = `${original.slice(0, start)}${target}${original.slice(start + trimmed.length)}`;
        changed = true;
      }
    }
    const aria = root.getAttribute("aria-label") || "";
    const ariaMatch = aria.match(config.ariaPattern);
    const ariaRule = ariaMatch && matchingRule(root, config.ariaSource);
    if (ariaRule) {
      const target = renderDynamicTarget(ariaRule.target, ariaMatch);
      root.setAttribute("aria-label", target);
      label.setAttribute("aria-label", target);
      changed = true;
    }
    return changed;
  }

  function collectTextNode(node) {
    if (!collectUntranslated || !node || node.nodeType !== Node.TEXT_NODE) return;
    const element = node.parentElement;
    if (isProtected(element) || !element?.closest?.(collectorSelector)) return;
    const source = (node.nodeValue || "").replace(/\s+/g, " ").trim();
    if (!source || source.length > 160 || !/[A-Za-z]/.test(source)) return;
    if (/https?:\/\/|www\.|[A-Za-z]:[\\/]|[/\\]{2,}|\S+@\S+/.test(source)) return;
    if (rulesBySource.has(source)) return;
    const tag = element.tagName.toLowerCase();
    const role = element.getAttribute("role") || element.closest?.("[role]")?.getAttribute("role") || "";
    const key = `${source}\u0000${tag}\u0000${role}`;
    untranslated.set(key, { source, tag, role });
  }

  function translateElement(element) {
    if (!(element instanceof Element)) return;
    replaceDynamicStatus(element);
    if (replaceCompositeLabel(element)) return;
    if (!isProtected(element)) {
      const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
      let node;
      while ((node = walker.nextNode())) {
        replaceTextNode(node);
        collectTextNode(node);
      }
    }
    for (const attribute of attributes) {
      const original = element.getAttribute(attribute);
      if (!original) continue;
      const rule = matchingRule(element, original.trim(), attribute === "placeholder");
      if (rule) element.setAttribute(attribute, rule.target);
    }
  }

  function scan(root) {
    if (!root) return;
    if (root.nodeType === Node.TEXT_NODE) {
      replaceTextNode(root);
      return;
    }
    if (!(root instanceof Element || root instanceof Document || root instanceof DocumentFragment)) return;
    if (root instanceof Element) translateElement(root);
    const selectors = [...new Set(rules.flatMap((rule) => rule.scopes))].join(",");
    if (!selectors) return;
    root.querySelectorAll?.(selectors).forEach(translateElement);
  }

  const queue = new Set();
  let scheduled = false;
  function schedule(node) {
    queue.add(node);
    if (scheduled) return;
    scheduled = true;
    queueMicrotask(() => {
      scheduled = false;
      for (const pending of queue) scan(pending);
      queue.clear();
    });
  }

  const observer = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      if (mutation.type === "characterData") {
        schedule(mutation.target.parentElement?.closest?.(compositeLabelSelector) || mutation.target);
      }
      if (mutation.type === "attributes") schedule(mutation.target);
      for (const added of mutation.addedNodes || []) {
        const parent = added.parentElement;
        schedule(
          parent?.closest?.(compositeLabelSelector)
          || parent?.closest?.(dynamicStatusSelector)
          || added
        );
      }
    }
  });
  globalThis.__cursorZhGetUntranslated = () => [...untranslated.values()];

  function start() {
    const root = document.documentElement || document.body;
    if (!root) {
      setTimeout(start, 25);
      return;
    }
    scan(root);
    observer.observe(root, {
      childList: true,
      subtree: true,
      characterData: true,
      attributes: true,
      attributeFilter: attributes
    });
  }

  start();
  return true;
})()

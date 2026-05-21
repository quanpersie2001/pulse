export function parseCliArgs(argv = []) {
  const options = new Map();
  const positionals = [];

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    if (!String(token).startsWith("--")) {
      positionals.push(token);
      continue;
    }

    const equalsIndex = token.indexOf("=");
    const flagName = equalsIndex === -1 ? token.slice(2) : token.slice(2, equalsIndex);
    let value = equalsIndex === -1 ? undefined : token.slice(equalsIndex + 1);
    if (value === undefined) {
      const next = argv[index + 1];
      if (next && !String(next).startsWith("--")) {
        value = next;
        index += 1;
      } else {
        value = true;
      }
    }

    const current = options.get(flagName);
    if (current === undefined) {
      options.set(flagName, value);
    } else if (Array.isArray(current)) {
      current.push(value);
    } else {
      options.set(flagName, [current, value]);
    }
  }

  return {
    options,
    positionals,
    optionNames: [...options.keys()],
    has(name) {
      return options.has(name);
    },
    string(name, fallback = "") {
      const value = options.get(name);
      if (value === undefined) {
        return fallback;
      }
      if (Array.isArray(value)) {
        const last = value[value.length - 1];
        return typeof last === "boolean" ? fallback : String(last);
      }
      return typeof value === "boolean" ? fallback : String(value);
    },
    boolean(name) {
      return Boolean(options.has(name) ? options.get(name) : false);
    },
    list(name) {
      const value = options.get(name);
      if (value === undefined) {
        return [];
      }
      return Array.isArray(value) ? value : [value];
    },
  };
}

export function assertKnownOptions(parsed, allowed) {
  const allowedOptions = new Set(allowed);
  for (const name of parsed.optionNames) {
    if (!allowedOptions.has(name)) {
      throw new Error(`Unknown argument: --${name}`);
    }
  }
}

export function assertBareBooleanOptions(parsed, names) {
  for (const name of names) {
    if (!parsed.has(name)) {
      continue;
    }
    const value = parsed.options.get(name);
    if (value !== true) {
      throw new Error(`Unknown argument: --${name}=${value}`);
    }
  }
}

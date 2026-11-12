import { describe, expect, it } from "vitest";
import { ROUTE_TABLE_PATTERN, loadedDeps, routeTable, scriptSource, staticClosure } from "./route-preload.mjs";

const chunk = (fileName, imports = [], extra = {}) => ({
  type: "chunk",
  fileName,
  imports,
  moduleIds: [],
  facadeModuleId: null,
  isEntry: false,
  viteMetadata: { importedCss: new Set() },
  ...extra,
});

// The startup (entry + runtime), a screen chunk that shares one helper chunk with the home screen
// and imports the entry, and a CSS file of the helper.
const bundle = {
  "assets/index-a.js": chunk("assets/index-a.js", ["assets/runtime-b.js"], { isEntry: true }),
  "assets/runtime-b.js": chunk("assets/runtime-b.js"),
  "assets/Desk-c.js": chunk("assets/Desk-c.js", ["assets/runtime-b.js", "assets/index-a.js", "assets/common-d.js"], {
    facadeModuleId: "E:\\repo\\apps\\web\\src\\features\\front-desk\\FrontDeskPage.tsx",
  }),
  "assets/common-d.js": chunk("assets/common-d.js", ["assets/index-a.js"], {
    viteMetadata: { importedCss: new Set(["assets/common-e.css"]) },
  }),
  "assets/Home-f.js": chunk("assets/Home-f.js", ["assets/index-a.js", "assets/common-d.js"], {
    moduleIds: ["/repo/apps/web/src/app/HomePage.tsx", "/repo/apps/web/src/app/BrandLogo.tsx"],
  }),
};
const routes = {
  "/quay": "/src/features/front-desk/FrontDeskPage.tsx",
  "/": "/src/app/HomePage.tsx",
};

describe("route preload table", () => {
  it("follows static imports only, each file once", () => {
    expect([...staticClosure(bundle, "assets/Desk-c.js")]).toEqual([
      "assets/Desk-c.js",
      "assets/runtime-b.js",
      "assets/index-a.js",
      "assets/common-d.js",
    ]);
  });

  it("lists each screen's chunks and their CSS, minus what the startup already loads", () => {
    expect(routeTable(bundle, routes)).toEqual({
      "/quay": ["Desk-c.js", "common-d.js", "common-e.css"],
      "/": ["Home-f.js", "common-d.js", "common-e.css"],
    });
  });

  it("refuses a screen that is missing or still in the startup chunk", () => {
    expect(() => routeTable(bundle, { "/x": "/src/app/Missing.tsx" })).toThrow(/no chunk holds/);
    const startup = { ...bundle, "assets/index-a.js": { ...bundle["assets/index-a.js"], moduleIds: ["/src/app/Shell.tsx"] } };
    expect(() => routeTable(startup, { "/": "/src/app/Shell.tsx" })).toThrow(/startup chunk/);
  });
});

/** Runs the emitted script on `path` and returns the links it added. */
function run(source, path) {
  const added = [];
  const document = {
    createElement: () => ({}),
    head: { append: (link) => added.push(link) },
  };
  new Function("location", "document", source)({ pathname: path }, document);
  return added;
}

describe("route preload script", () => {
  const table = { "/quay": ["Desk-c.js", "common-d.js", "common-e.css"], "/": ["Home-f.js"] };
  const source = scriptSource(table, "/assets/");

  it("modulepreloads the landing screen's chunks and preloads its CSS, like Vite's own links", () => {
    expect(run(source, "/quay")).toEqual([
      { rel: "modulepreload", crossOrigin: "", href: "/assets/Desk-c.js" },
      { rel: "modulepreload", crossOrigin: "", href: "/assets/common-d.js" },
      { rel: "preload", as: "style", crossOrigin: "", href: "/assets/common-e.css" },
    ]);
  });

  it("treats a trailing slash as the same screen, the root as home, other paths as nothing", () => {
    expect(run(source, "/quay/").map((l) => l.href)).toHaveLength(3);
    expect(run(source, "/").map((l) => l.href)).toEqual(["/assets/Home-f.js"]);
    expect(run(source, "/khach-hang")).toEqual([]);
  });

  it("carries the table where the budget gate can read it back", () => {
    expect(JSON.parse(ROUTE_TABLE_PATTERN.exec(source)?.[1] ?? "null")).toEqual(table);
  });
});

describe("lazy import preload lists", () => {
  it("drop the files the importing chunk already loaded, keep the import itself", () => {
    const { plugin, resolveDependencies } = loadedDeps();
    plugin.generateBundle.handler({}, bundle);
    expect(
      resolveDependencies("assets/Home-f.js", ["assets/Home-f.js", "assets/index-a.js", "assets/common-d.js"], {
        hostId: "assets/Desk-c.js",
        hostType: "js",
      }),
    ).toEqual(["assets/Home-f.js"]);
    // From the startup chunk, the shared chunk is not loaded yet: it stays.
    expect(
      resolveDependencies("assets/Home-f.js", ["assets/Home-f.js", "assets/common-d.js", "assets/runtime-b.js"], {
        hostId: "assets/index-a.js",
        hostType: "js",
      }),
    ).toEqual(["assets/Home-f.js", "assets/common-d.js"]);
    expect(resolveDependencies("x.js", ["x.js", "assets/index-a.js"], { hostId: "index.html", hostType: "html" })).toEqual([
      "x.js",
      "assets/index-a.js",
    ]);
  });
});

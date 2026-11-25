import test from "node:test";

import TemplateMap from "../../../src/TemplateMap.js";
import getNewTemplateForTests from "../../_getNewTemplateForTests.js";
import { getTemplateConfigInstance } from "../../_testHelpers.js";

function getNewTemplate(filename, input, output, twelvetyConfig) {
  return getNewTemplateForTests(filename, input, output, null, null, twelvetyConfig);
}

test("Get ordered list of templates", async (t) => {
  let twelvetyConfig = await getTemplateConfigInstance({
		dir: {
			input: "test/_issues/975/",
			output: "test/_issues/975/_site",
		}
	});

  let tm = new TemplateMap(twelvetyConfig);

  // These two templates are add-order-dependent
  await tm.add(
    await getNewTemplate(
      "./test/_issues/975/post.md",
      "./test/_issues/975/",
      "./test/_issues/975/_site",
      twelvetyConfig
    )
  );

  await tm.add(
    await getNewTemplate(
      "./test/_issues/975/another-post.md",
      "./test/_issues/975/",
      "./test/_issues/975/_site",
      twelvetyConfig
    )
  );

  // This template should be last
  await tm.add(
    await getNewTemplate(
      "./test/_issues/975/index.md",
      "./test/_issues/975/",
      "./test/_issues/975/_site",
      twelvetyConfig
    )
  );

  await tm.cache();

  let order = tm.getTemplateOrder();
  t.assert.deepStrictEqual(order, [
    "./test/_issues/975/post.md",
    "./test/_issues/975/another-post.md",
    "__collection:post",
    "./test/_issues/975/index.md",
    "__collection:[keys]",
    "__collection:all",
  ]);
});

test("Get ordered list of templates (reverse add)", async (t) => {
	let twelvetyConfig = await getTemplateConfigInstance({
		dir: {
			input: "test/_issues/975/",
			output: "test/_issues/975/_site",
		}
	});

  let tm = new TemplateMap(twelvetyConfig);

  // This template is now first
  await tm.add(
    await getNewTemplate(
      "./test/_issues/975/index.md",
      "./test/_issues/975/",
      "./test/_issues/975/_site",
      twelvetyConfig
    )
  );

  // These two templates are add-order-dependent
  await tm.add(
    await getNewTemplate(
      "./test/_issues/975/another-post.md",
      "./test/_issues/975/",
      "./test/_issues/975/_site",
      twelvetyConfig
    )
  );

  await tm.add(
    await getNewTemplate(
      "./test/_issues/975/post.md",
      "./test/_issues/975/",
      "./test/_issues/975/_site",
      twelvetyConfig
    )
  );

  await tm.cache();

  let order = tm.getTemplateOrder();
  t.assert.deepStrictEqual(order, [
    "./test/_issues/975/another-post.md",
    "./test/_issues/975/post.md",
    "__collection:post",
    "./test/_issues/975/index.md",
    "__collection:[keys]",
    "__collection:all",
  ]);
});

export default function (twelvetyConfig) {
	twelvetyConfig.addPassthroughCopy("test/stubs/events-file-passthrough/robots.txt");
	twelvetyConfig.addPassthroughCopy({ "test/stubs/events-file-passthrough/img": "img" });
}

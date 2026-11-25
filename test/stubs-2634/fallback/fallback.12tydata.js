export default {
  twelvetyComputed: {
    permalink: function (data) {
      return data.permalink || `/fallback/${this.slugify(data.title)}/`;
    },
  },
};

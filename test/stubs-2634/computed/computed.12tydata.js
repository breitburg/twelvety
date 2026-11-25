export default {
  twelvetyComputed: {
    permalink: function (data) {
      return `/computed/${this.slugify(data.title)}/`;
    },
  },
};

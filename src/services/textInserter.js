const { keyboard } = require("@nut-tree-fork/nut-js");

async function insertTranscript(text) {
  if (!text || !text.trim()) {
    return;
  }
  await keyboard.type(text);
}

module.exports = {
  insertTranscript
};

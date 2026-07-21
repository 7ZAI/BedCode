async function activate(context) {
  context.logger.info("Auto Task plugin activating...");
  context.ui.registerTerminalToolbarItem({
    id: "auto-task-toolbar",
    label: context.i18n.t("mobile.autoTask.title"),
    icon: "📋",
    onClick: () => {
    }
  });
  context.logger.info("Auto Task plugin activated");
}
async function deactivate() {
  console.log("[Auto Task] Plugin deactivated");
}
export {
  activate,
  deactivate
};

// Whether someone is signed in, as the cookie account.rs keeps in step with the session says: the
// session's own is out of scripts' reach. Without one there is nothing to ask the server for, the
// listings the hearts keep (heart_controller.js) or unread messages (inbox_controller.js), and the
// page skips asking.
export function signedIn() {
  return document.cookie.split("; ").includes("llogari=1");
}

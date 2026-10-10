import { t } from "controllers/i18n.js";

// Not a controller: where the visitor is, as the browser says once they let it, for "Afër meje"
// (place_controller.js, filters_controller.js) and the map's "Vendndodhja ime" (map_controller.js).
// The point stays in the page: it goes to the server only as the search's point. Rejects with what
// to tell the visitor, in the page's language.
export function locate() {
  return new Promise((resolve, reject) => {
    if (!navigator.geolocation) {
      reject(new Error(t("Ky shfletues nuk e jep vendndodhjen. Zgjidh qytetin.")));
      return;
    }
    navigator.geolocation.getCurrentPosition(
      ({ coords }) => resolve({ latitude: coords.latitude, longitude: coords.longitude }),
      () => reject(new Error(t("Vendndodhja nuk u gjet. Lejoje te shfletuesi, ose zgjidh qytetin."))),
      { enableHighAccuracy: false, timeout: 10000, maximumAge: 300000 },
    );
  });
}

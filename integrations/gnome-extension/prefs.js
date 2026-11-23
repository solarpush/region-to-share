import Adw from 'gi://Adw';
import Gio from 'gi://Gio';
import { ExtensionPreferences, gettext as _ } from 'resource:///org/gnome/Shell/Extensions/js/extensions/prefs.js';

export default class RegionToSharePreferences extends ExtensionPreferences {
    fillPreferencesWindow(window) {
        const settings = this.getSettings();
        const page = new Adw.PreferencesPage({
            title: _('General'),
            icon_name: 'preferences-system-symbolic',
        });
        window.add(page);

        const group = new Adw.PreferencesGroup({
            title: _('Top Bar & Quick Settings'),
            description: _('Configure how Region to Share appears in GNOME Shell.'),
        });
        page.add(group);

        const pillRow = new Adw.SwitchRow({
            title: _('Show Live Top Bar Pill'),
            subtitle: _('Displays a live LIVE / REC timer indicator in the top panel while active'),
        });
        settings.bind('show-topbar-pill', pillRow, 'active', Gio.SettingsBindFlags.DEFAULT);
        group.add(pillRow);
    }
}

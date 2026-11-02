// MăngRS Focus — names the focused window over D-Bus.
//
// GNOME reports Wayland-native applications to IBus engines only as an
// anonymous mutter relay, and every window-list API outside the shell is
// locked down on modern GNOME: Shell Introspect denies regular processes
// (verified on 51) and the wlroots toplevel protocol ibus-bamboo uses is
// not implemented by Mutter. An extension runs inside the shell, where
// global.display.focus_window and the window's wm_class are freely
// available — so it exposes them on the session bus and the engine asks
// for the name on every unnamed or relayed focus change.

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const XML = `
<node>
  <interface name="io.github.fundokukiri.Mangrs">
    <method name="GetFocusedWindow">
      <arg direction="out" type="s" name="wm_class"/>
      <arg direction="out" type="s" name="title"/>
    </method>
  </interface>
</node>`;

const OBJ_PATH = '/io/github/fundokukiri/Mangrs';
const IFACE = 'io.github.fundokukiri.Mangrs';

export default class MangrsFocusExtension extends Extension {
    enable() {
        this._nodeInfo = Gio.DBusNodeInfo.new_for_xml(XML);
        this._conn = null;
        this._registrationId = 0;
        // REPLACE: a stale shell instance (fast user-switch oddities) must
        // not keep the name away from the live one.
        this._ownerId = Gio.bus_own_name(
            Gio.BusType.SESSION,
            IFACE,
            Gio.BusNameOwnerFlags.REPLACE,
            (conn) => this._export(conn),
            null,
            null
        );
    }

    _export(conn) {
        this._conn = conn;
        // A second focus change can race the first export: report and
        // ignore instead of throwing inside the shell.
        try {
            this._registrationId = conn.register_object(
                OBJ_PATH,
                this._nodeInfo.lookup_interface(IFACE),
                {
                    handle_method_call: (
                        c,
                        sender,
                        path,
                        iface,
                        method,
                        params,
                        invocation
                    ) => {
                        if (method !== 'GetFocusedWindow') {
                            invocation.return_error_literal(
                                Gio.DBusError.UNKNOWN_METHOD,
                                `${iface}.${method}: unknown method`
                            );
                            return;
                        }
                        // No focused window (the overview, a shell popup)
                        // means no name — the engine falls back gracefully.
                        const win = global.display.focus_window;
                        const wmClass = win ? win.get_wm_class() || '' : '';
                        const title = win ? win.get_title() || '' : '';
                        invocation.return_value(
                            new GLib.Variant('(ss)', [wmClass, title])
                        );
                    },
                }
            );
        } catch (e) {
            console.warn(`mangrs-focus: object export failed: ${e}`);
        }
    }

    disable() {
        if (this._conn && this._registrationId) {
            this._conn.unexport_object(OBJ_PATH);
        }
        this._registrationId = 0;
        this._conn = null;
        if (this._ownerId !== undefined) {
            Gio.bus_unown_name(this._ownerId);
            this._ownerId = undefined;
        }
        this._nodeInfo = null;
    }
}

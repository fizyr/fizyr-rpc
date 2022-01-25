# Wireshark dissector

This folder contains a Wireshark dissector for the Fizyr RPC protocol.
Copy the `fizyr-rpc.lua` file to your Wireshark plugin directory to install it.
See [the Wireshark manual][wireshark-plugin-folder] for more information on the plugin folder.

Afterwards, either restart Wireshark or select the "Reload LUA plugins" option from the "Analyze" menu.
You should now be able to activate the dissector from "Decode As ..." dialog from the "Analyze" menu.

[wireshark-plugin-folder]: https://www.wireshark.org/docs/wsug_html_chunked/ChPluginFolders.html

# Privacy

Bloc doesn't collect anything and doesn't send anything on its own. It has no analytics, no telemetry and no account.

What it keeps stays on your PC, in `%APPDATA%\Bloc`: your settings (`bloc.ini`), the list of recent files and an error
log (`errores.log`) that only Bloc reads.

## Reporting a bug

File › Report a problem opens a short form. When you press Send, Bloc opens the "new issue" page of this repository in your
browser with the report already filled in. Nothing leaves your PC until you send that page yourself, with your own
GitHub account. You can read the whole text first with "See what is sent", and edit it in the browser.

The technical details are optional: untick "Include technical details" to leave them out. They never include the text of your documents, file names or
paths. They are:

- the Bloc and Windows versions, language, display scale and zoom;
- night mode, marks and word wrap;
- the type of the open document (.md or .txt), its rounded size and how many tables it has;
- Bloc's options, without the recent files;
- the last 50 lines of the error log, with every path and file name replaced by `<archivo>.ext`.

## Updates

Bloc only updates itself from the folder you installed it from, and only with a copy signed with the project's key. It
never downloads anything.

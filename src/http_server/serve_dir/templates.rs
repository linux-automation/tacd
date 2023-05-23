// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2023 Pengutronix e.K.
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.

pub const DIR_LISTING: &str = r#"<!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8" />
    <link rel="icon" href="/favicon.ico" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <meta name="description" content="Directory Listing" />
    <link rel="apple-touch-icon" href="/logo192.png" />
    <title>Index of <DIR_NAME/></title>
    <style>
      * {
        margin: 0;
        padding: 0;
      }

      a {
        text-decoration: none;
      }

      a:hover {
        text-decoration: underline;
      }

      a:visited {
        color: black;
      }

      h2 {
        color: #606060;
        margin-top: 0.5em;
        margin-bottom: 0.5em;
      }

      img {
        float: right;
        width: 7em;
      }

      main {
        background-color: #fbfbfb;
        box-shadow: 0 0 1em #00000012;
        margin: 0 auto;
        max-width: 60em;
        min-height: 100vh;
        padding: 2em;
        width: 100%;
      }

      table {
        margin-top: 7em;
        width: 100%;
      }

      td {
        text-align: right;
        padding: 0.2em 0.5em;
      }

      td:first-child {
        text-align: left;
      }

      tr:nth-child(even) {
        background-color: #00000012;
      }
    </style>
  </head>
  <body>
    <main>
      <img src="/logo.svg" />
      <h1>Index of</h1>
      <h2><DIR_NAME/></h2>
      <a href="/">Back to the web interface</a>
      <table>
        <tr>
          <th>Name</th>
          <th>Last modified</th>
          <th>Size</th>
        </tr>
        <TABLE_ROWS/>
      </table>
    </main>
  </body>
</html>
"#;

pub const NOT_FOUND: &str = r#"<!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8" />
    <link rel="icon" href="/favicon.ico" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <meta name="description" content="File not found" />
    <link rel="apple-touch-icon" href="/logo192.png" />
    <title>404 Not Found</title>
    <style>
      a:visited {
        color: black;
      }

      img {
        width: 10em;
      }

      main {
        background-color: #fbfbfb;
        border-radius: 2em;
        box-shadow: 0 0 1em #00000045;
        left: 50%;
        max-width: 50em;
        padding: 2em;
        position: absolute;
        text-align: center;
        top: 50%;
        transform: translate(-50%,-50%);
      }
    </style>
  </head>
  <body>
    <main>
      <img src="/logo.svg" />
      <h1>404 Not Found</h1>
      <p>Sorry. I could not find what you are looking for.</p>
      <a href="/">Go back to the user interface?</a>
    </main>
  </body>
</html>
"#;

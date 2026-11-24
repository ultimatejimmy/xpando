/**
 * Clipboard HTML as browsers hand it to a page (`text/html`): the fragment wrapped in
 * html/body, with Word's own markers. Written by hand from what Word 365, Outlook, Excel and
 * Google Docs put on the clipboard. Used by the paste tests; not shipped (nothing imports it).
 */

const WORD_HEAD = `<html xmlns:v="urn:schemas-microsoft-com:vml"
xmlns:o="urn:schemas-microsoft-com:office:office"
xmlns:w="urn:schemas-microsoft-com:office:word"
xmlns:m="http://schemas.microsoft.com/office/2004/12/omml"
xmlns="http://www.w3.org/TR/REC-html40">

<head>
<meta http-equiv=Content-Type content="text/html; charset=utf-8">
<meta name=ProgId content=Word.Document>
<meta name=Generator content="Microsoft Word 15">
<meta name=Originator content="Microsoft Word 15">
<!--[if gte mso 9]><xml>
 <o:OfficeDocumentSettings>
  <o:AllowPNG/>
 </o:OfficeDocumentSettings>
</xml><![endif]-->
<style>
<!--
 /* Font Definitions */
 @font-face
	{font-family:"Cambria Math";
	panose-1:2 4 5 3 5 4 6 3 2 4;}
@font-face
	{font-family:Calibri;
	panose-1:2 15 5 2 2 2 4 3 2 4;}
 /* Style Definitions */
 p.MsoNormal, li.MsoNormal, div.MsoNormal
	{mso-style-unhide:no;
	mso-style-qformat:yes;
	margin-top:0in;
	margin-right:0in;
	margin-bottom:8.0pt;
	margin-left:0in;
	line-height:107%;
	font-size:11.0pt;
	font-family:"Calibri",sans-serif;}
-->
</style>
</head>

<body lang=EN-US style='tab-interval:.5in;word-wrap:break-word'>
<!--StartFragment-->`;

const WORD_TAIL = `<!--EndFragment-->
</body>

</html>`;

const word = (fragment: string) => `${WORD_HEAD}${fragment}${WORD_TAIL}`;

/** Coloured and highlighted runs, Calibri 11pt, a centred heading, smart quotes, spacing runs. */
export const WORD_FORMATTED = word(`<h1 align=center style='text-align:center'><span style='font-size:20.0pt;line-height:107%;font-family:"Calibri Light",sans-serif;color:#2F5496'>Monthly&nbsp;report<o:p></o:p></span></h1>

<p class=MsoNormal><span style='font-size:11.0pt;line-height:107%;font-family:"Calibri",sans-serif'>Dear </span><b><span style='font-size:11.0pt;line-height:107%;font-family:"Calibri",sans-serif;color:#C00000'>Customer</span></b><span style='font-size:11.0pt;line-height:107%;font-family:"Calibri",sans-serif'>,<o:p></o:p></span></p>

<p class=MsoNormal><span style='font-size:11.0pt;line-height:107%;font-family:"Calibri",sans-serif'>It&#8217;s <i>very</i> <u>important</u> that you &#8220;read this&#8221;<span style='mso-spacerun:yes'>&nbsp; </span>today &ndash; <span style='background:yellow;mso-highlight:yellow'>highlighted</span>, <s>old</s> and H<sub>2</sub>O, x<sup>2</sup>.<o:p></o:p></span></p>

<p class=MsoNormal align=right style='text-align:right;margin-left:.5in;line-height:150%'><span style='font-size:9.0pt;font-family:"Arial",sans-serif'>Visit <a href="https://example.com/a?b=1&amp;c=2"><span style='font-family:"Arial",sans-serif;color:#0563C1'>our site</span></a> or <a href="mailto:help@example.com">email us</a>.<o:p></o:p></span></p>

<p class=MsoNormal><o:p>&nbsp;</o:p></p>

<p class=MsoNormal><span style='font-size:11.0pt'>Last line<o:p></o:p></span></p>`);

/** Bulleted list with a nested level and a numbered list, as Word's fake MsoListParagraph paragraphs. */
export const WORD_LISTS = word(`<p class=MsoListParagraphCxSpFirst style='text-indent:-.25in;mso-list:l0 level1 lfo1'><![if !supportLists]><span style='font-family:Symbol;mso-fareast-font-family:Symbol;mso-bidi-font-family:Symbol'><span style='mso-list:Ignore'>&middot;<span style='font:7.0pt "Times New Roman"'>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; </span></span></span><![endif]>First item<o:p></o:p></p>

<p class=MsoListParagraphCxSpMiddle style='margin-left:1.0in;text-indent:-.25in;mso-list:l0 level2 lfo1'><![if !supportLists]><span style='font-family:"Courier New";mso-fareast-font-family:"Courier New"'><span style='mso-list:Ignore'>o<span style='font:7.0pt "Times New Roman"'>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; </span></span></span><![endif]>Nested one<o:p></o:p></p>

<p class=MsoListParagraphCxSpMiddle style='margin-left:1.0in;text-indent:-.25in;mso-list:l0 level2 lfo1'><![if !supportLists]><span style='font-family:"Courier New"'><span style='mso-list:Ignore'>o<span style='font:7.0pt "Times New Roman"'>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; </span></span></span><![endif]>Nested <b>two</b><o:p></o:p></p>

<p class=MsoListParagraphCxSpLast style='text-indent:-.25in;mso-list:l0 level1 lfo1'><![if !supportLists]><span style='font-family:Symbol'><span style='mso-list:Ignore'>&middot;<span style='font:7.0pt "Times New Roman"'>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; </span></span></span><![endif]>Second item<o:p></o:p></p>

<p class=MsoNormal>Steps:<o:p></o:p></p>

<p class=MsoListParagraphCxSpFirst style='text-indent:-.25in;mso-list:l1 level1 lfo2'><![if !supportLists]><span><span style='mso-list:Ignore'>1.<span style='font:7.0pt "Times New Roman"'>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; </span></span></span><![endif]>Open it<o:p></o:p></p>

<p class=MsoListParagraphCxSpLast style='text-indent:-.25in;mso-list:l1 level1 lfo2'><![if !supportLists]><span><span style='mso-list:Ignore'>2.<span style='font:7.0pt "Times New Roman"'>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; </span></span></span><![endif]>Press <i>go</i><o:p></o:p></p>`);

/** A shaded, bordered table with a merged header cell and a merged column. */
export const WORD_TABLE = word(`<table class=MsoTableGrid border=1 cellspacing=0 cellpadding=0 width=468 style='width:351.0pt;border-collapse:collapse;border:none;mso-border-alt:solid windowtext .5pt;mso-yfti-tbllook:1184;mso-padding-alt:0in 5.4pt 0in 5.4pt'>
 <tr style='mso-yfti-irow:0;mso-yfti-firstrow:yes'>
  <td width=468 colspan=2 valign=top style='width:351.0pt;border:solid windowtext 1.0pt;mso-border-alt:solid windowtext .5pt;background:#D9E2F3;padding:0in 5.4pt 0in 5.4pt'>
  <p class=MsoNormal align=center style='margin-bottom:0in;text-align:center;line-height:normal'><b>Quarter</b><o:p></o:p></p>
  </td>
 </tr>
 <tr style='mso-yfti-irow:1'>
  <td width=234 rowspan=2 valign=top style='width:175.5pt;border:solid windowtext 1.0pt;border-top:none;mso-border-top-alt:solid windowtext .5pt;mso-border-alt:solid windowtext .5pt;padding:0in 5.4pt 0in 5.4pt'>
  <p class=MsoNormal style='margin-bottom:0in;line-height:normal'>Q1<o:p></o:p></p>
  </td>
  <td width=234 valign=top style='width:175.5pt;border-top:none;border-left:none;border-bottom:solid windowtext 1.0pt;border-right:solid windowtext 1.0pt;background:#FFF2CC;padding:0in 5.4pt 0in 5.4pt'>
  <p class=MsoNormal style='margin-bottom:0in;line-height:normal'><span style='color:red'>10&nbsp;%</span><o:p></o:p></p>
  </td>
 </tr>
 <tr style='mso-yfti-irow:2;mso-yfti-lastrow:yes'>
  <td width=234 valign=top style='width:175.5pt;border-top:none;border-left:none;border-bottom:solid windowtext 1.0pt;border-right:solid windowtext 1.0pt;padding:0in 5.4pt 0in 5.4pt'>
  <p class=MsoNormal style='margin-bottom:0in;line-height:normal'>12 %<o:p></o:p></p>
  </td>
 </tr>
</table>

<p class=MsoNormal><o:p>&nbsp;</o:p></p>`);

/** A reporting-template tag typed as text: Word escapes it, and it must stay literal text. */
export const WORD_LITERAL_TAG = word(
  `<p class=MsoNormal><span style='font-family:"Courier New"'>&lt;wr:forEach select="/Report/Survey"&gt;</span><o:p></o:p></p>

<p class=MsoNormal>Name: ${"{{name}}"} &amp; &lt;b&gt;not bold&lt;/b&gt; 5 &lt; 6<o:p></o:p></p>

<p class=MsoNormal>&lt;/wr:forEach&gt;<o:p></o:p></p>`,
);

/** A picture on the user's disk and one from the web; comments and footnote markers. */
export const WORD_IMAGES = word(`<p class=MsoNormal>Logo: <span style='mso-no-proof:yes'><!--[if gte vml 1]><v:shape id="Picture_x0020_1" o:spid="_x0000_i1025" type="#_x0000_t75" style='width:100pt;height:50pt'><v:imagedata src="file:///C:/Users/me/AppData/Local/Temp/msohtmlclip1/01/clip_image001.png" o:title=""/></v:shape><![endif]--><![if !vml]><img width=133 height=67 src="file:///C:/Users/me/AppData/Local/Temp/msohtmlclip1/01/clip_image001.png" v:shapes="Picture_x0020_1"><![endif]></span><o:p></o:p></p>

<p class=MsoNormal>Mail: <img width=20 height=20 src="cid:image002.png@01D9ABCD.1234" alt="cid"> and web: <img width=40 height=20 src="https://example.com/logo.png" alt="Logo"><o:p></o:p></p>`);

/** A single Excel cell and a small range. */
export const EXCEL_CELL = `<html xmlns:o="urn:schemas-microsoft-com:office:office"
xmlns:x="urn:schemas-microsoft-com:office:excel"
xmlns="http://www.w3.org/TR/REC-html40">
<head>
<meta http-equiv=Content-Type content="text/html; charset=utf-8">
<meta name=ProgId content=Excel.Sheet>
<style>
<!--table
	{mso-displayed-decimal-separator:"\\.";}
td
	{padding-top:1px;mso-ignore:padding;color:black;font-size:11.0pt;font-family:Calibri, sans-serif;white-space:nowrap;}
-->
</style>
</head>
<body link="#0563C1" vlink="#954F72">
<table border=0 cellpadding=0 cellspacing=0 width=64 style='border-collapse:collapse;width:48pt'>
<!--StartFragment-->
 <col width=64 style='width:48pt'>
 <tr height=20 style='height:15.0pt'>
  <td height=20 class=xl65 width=64 style='height:15.0pt;width:48pt'>Hello cell</td>
 </tr>
<!--EndFragment-->
</table>
</body>
</html>`;

export const EXCEL_RANGE = `<html><body>
<table border=0 cellpadding=0 cellspacing=0 width=128 style='border-collapse:collapse;width:96pt'>
<!--StartFragment-->
 <tr height=20 style='height:15.0pt'>
  <td height=20 width=64 style='height:15.0pt;width:48pt'>A1</td>
  <td width=64 style='width:48pt;background:#C6E0B4;color:#375623;font-weight:700'>B1</td>
 </tr>
 <tr height=20 style='height:15.0pt'>
  <td height=20 align=right style='height:15.0pt'>1,5</td>
  <td></td>
 </tr>
<!--EndFragment-->
</table>
</body></html>`;

/** Google Docs: the whole paragraph is inside a `<b style="font-weight:normal">` wrapper. */
export const GOOGLE_DOCS = `<meta charset='utf-8'><meta charset="utf-8"><b style="font-weight:normal;" id="docs-internal-guid-1a2b3c4d-7fff-1234-5678-abcdef012345"><h2 dir="ltr" style="line-height:1.38;margin-top:18pt;margin-bottom:6pt;"><span style="font-size:16pt;font-family:Arial,sans-serif;color:#000000;background-color:transparent;font-weight:400;font-style:normal;font-variant:normal;text-decoration:none;vertical-align:baseline;white-space:pre;white-space:pre-wrap;">Heading</span></h2><p dir="ltr" style="line-height:1.38;text-align:justify;margin-top:0pt;margin-bottom:0pt;"><span style="font-size:11pt;font-family:Arial,sans-serif;color:#000000;background-color:#ffff00;font-weight:700;font-style:italic;text-decoration:underline;-webkit-text-decoration-skip:none;text-decoration-skip-ink:none;vertical-align:baseline;white-space:pre;white-space:pre-wrap;">Bold italic underlined yellow</span><span style="font-size:11pt;font-family:Arial,sans-serif;color:#000000;font-weight:400;">, then plain.</span></p><ul style="margin-top:0;margin-bottom:0;padding-inline-start:48px;"><li dir="ltr" style="list-style-type:disc;font-size:11pt;font-family:Arial,sans-serif;" aria-level="1"><p dir="ltr" style="line-height:1.38;margin-top:0pt;margin-bottom:0pt;" role="presentation"><span style="font-size:11pt;font-family:Arial,sans-serif;color:#000000;">One</span></p></li><li dir="ltr" style="list-style-type:disc;" aria-level="1"><p dir="ltr" role="presentation"><span style="font-size:11pt;">Two</span></p></li></ul></b>`;

/** Outlook (desktop) mail body: conditional comments, MsoNormal, Segoe UI. */
export const OUTLOOK = `<html xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:w="urn:schemas-microsoft-com:office:word">
<head><meta http-equiv="Content-Type" content="text/html; charset=utf-8">
<!--[if gte mso 9]><xml><o:shapedefaults v:ext="edit" spidmax="1026" /></xml><![endif]-->
<style>@font-face {font-family:"Segoe UI";} p.MsoNormal {margin:0in;}</style></head>
<body lang="EN-US" link="#0563C1" vlink="#954F72" style="word-wrap:break-word">
<div class="WordSection1">
<!--StartFragment--><p class="MsoNormal"><span style="font-family:&quot;Segoe UI&quot;,sans-serif;color:#1F3864">Hi team,</span></p>
<p class="MsoNormal"><span style="font-family:&quot;Segoe UI&quot;,sans-serif"><br>Best regards,<br>Alex</span></p><!--EndFragment-->
</div>
</body></html>`;

/** A web page selection: classes, ids, scripts, inline event handlers, a relative link. */
export const WEB_PAGE = `<html><body><!--StartFragment--><div class="post" id="p1" onclick="steal()"><h3 class="title">Release <em>notes</em></h3><p class="lead">Read the <a href="/docs/start" target="_blank">guide</a> or <a href="https://example.com/x" rel="noopener" onclick="x()">the site</a>.</p><script>alert(1)</script><ul class="list"><li>one</li><li>two <strong>bold</strong></li></ul><blockquote>Quoted</blockquote><pre>line 1
  line 2</pre><hr><img src="/relative.png" alt="x"></div><!--EndFragment--></body></html>`;

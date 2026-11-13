#!/usr/bin/env bash
# Makes this film's evidence again from nothing: the 26 news-page captures in assets/pages/ (PNG and
# JSON, with scripts/page-quote.mjs) and the six third-party clips footage/c01.mp4 to c06.mp4 (a
# yt-dlp section into source/clips/, then an ffmpeg conform). A step is skipped when its output exists.
# Pages change (wording, layout, adverts) and videos get re-encoded or removed, so a capture or a clip
# made later may differ from the one the film was cut with: look at each new file before using it.
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p assets/pages source/clips footage

PQ=../../scripts/page-quote.mjs
# Advert slots and promo boxes hidden on Kompas.com and on Detik pages.
KH=".kcm,.gate-kgplus,.ads-on-body,.ads-partner-wrap,.kgnw-modal-ads"
DH=".parallaxindetail,.staticdetail_container,.box-ads,.para_caption,.detikads"

# ---------------------------------------------------------------- page captures

# The demand: 18 years
[ -s assets/pages/kompastv-tuntutan-18-tahun.png ] || node "$PQ" \
  "https://www.kompas.tv/nasional/668664/nadiem-makarim-dituntut-18-tahun-penjara-dalam-kasus-chromebook" \
  "dengan pidana penjara selama 18 tahun." \
  -o assets/pages/kompastv-tuntutan-18-tahun.png

[ -s assets/pages/kompastv-tuntutan-18-tahun-top.png ] || node "$PQ" \
  "https://www.kompas.tv/nasional/668664/nadiem-makarim-dituntut-18-tahun-penjara-dalam-kasus-chromebook" \
  "Nadiem Makarim Dituntut 18 Tahun Penjara dalam Kasus Chromebook" \
  --top --hide ".ads-top,.jacket-none br" \
  -o assets/pages/kompastv-tuntutan-18-tahun-top.png

[ -s assets/pages/cnn-tuntutan-18-tahun.png ] || node "$PQ" \
  "https://www.cnnindonesia.com/nasional/20260513132823-12-1358220/nadiem-dituntut-18-tahun-penjara-di-kasus-korupsi-chromebook" \
  "dituntut dengan pidana 18 tahun penjara" \
  -o assets/pages/cnn-tuntutan-18-tahun.png

# The replacement money, and Nadiem saying he does not have it
[ -s assets/pages/detik-uang-pengganti-5-6-triliun.png ] || node "$PQ" \
  "https://news.detik.com/berita/d-8487973/nadiem-juga-dituntut-bayar-uang-pengganti-rp-5-6-triliun" \
  "uang pengganti yang harus dibayar Nadiem sebesar Rp 5.681.066.728.758" \
  --also "diganti dengan pidana kurungan selama 9 tahun." \
  --hide "$DH" \
  -o assets/pages/detik-uang-pengganti-5-6-triliun.png

# Kompas keeps this article's body for subscribers (blurred behind a gate): the quote comes from Antara.
[ -s assets/pages/antara-nadiem-tidak-punya-uang.png ] || node "$PQ" \
  "https://jogja.antaranews.com/berita/828424/nadiem-makarim-sakit-hati-dituntut-uang-pengganti-rp567-triliun" \
  "Mereka tahu saya tidak punya uang itu" \
  -o assets/pages/antara-nadiem-tidak-punya-uang.png

[ -s assets/pages/kompas-nadiem-tidak-punya-uang-top.png ] || node "$PQ" \
  "https://nasional.kompas.com/read/2026/05/14/12544281/dituntut-bayar-rp-568-triliun-nadiem-saya-tidak-punya-uang-itu" \
  "Dituntut Bayar Rp 5,68 Triliun, Nadiem: Saya Tidak Punya Uang Itu" \
  --top --hide "$KH" \
  -o assets/pages/kompas-nadiem-tidak-punya-uang-top.png

# Value of the corruption against the state loss
[ -s assets/pages/hukumonline-uang-pengganti-vs-kerugian-negara.png ] || node "$PQ" \
  "https://www.hukumonline.com/berita/a/saat-nadiem-dituntut-bayar-uang-pengganti-lebih-tinggi-dari-hitungan-kerugian-negara-lt6a06c37912e96/" \
  "antara tuntutan uang pengganti dan kerugian negara sebesar Rp3,5 triliun." \
  --also "kerugian keuangan negara dari perkara ini sebesar Rp2,1 triliun" \
  -o assets/pages/hukumonline-uang-pengganti-vs-kerugian-negara.png

[ -s assets/pages/antara-kerugian-negara-vs-uang-diterima.png ] || node "$PQ" \
  "https://www.antaranews.com/berita/5566955/nadiem-makarim-sakit-hati-dituntut-uang-pengganti-rp567-triliun" \
  "Nadiem diduga telah menerima uang sebesar Rp809,59 miliar" \
  --also "kerugian negara yang disebabkan meliputi sebesar Rp1,56 triliun" \
  --wait 7000 \
  -o assets/pages/antara-kerugian-negara-vs-uang-diterima.png

# The schedule: pleidoi, verdict date, and the verdict itself
[ -s assets/pages/kompas-pleidoi-2-juni.png ] || node "$PQ" \
  "https://nasional.kompas.com/read/2026/05/15/09551581/tunggu-pulih-dari-operasi-sidang-pembelaan-nadiem-makarim-dijadwalkan-pada-2" \
  "akan digelar pada 2 Juni 2026." \
  --hide "$KH" \
  -o assets/pages/kompas-pleidoi-2-juni.png

[ -s assets/pages/kompas-vonis-30-juni.png ] || node "$PQ" \
  "https://nasional.kompas.com/read/2026/06/23/17301121/nadiem-makarim-bakal-hadapi-vonis-kasus-chromebook-pada-30-juni" \
  "(Putusan) Selasa, 30 Juni 2026," \
  --hide "$KH" \
  -o assets/pages/kompas-vonis-30-juni.png

[ -s assets/pages/detik-vonis-10-tahun.png ] || node "$PQ" \
  "https://news.detik.com/berita/d-8554661/vonis-10-tahun-penjara-untuk-nadiem-atas-perbuatan-terencana" \
  "Majelis hakim menjatuhkan vonis 10 tahun penjara" \
  --hide "$DH" \
  -o assets/pages/detik-vonis-10-tahun.png

# Tuntutan against vonis (explainer)
[ -s assets/pages/detik-beda-tuntutan-vonis-tuntutan.png ] || node "$PQ" \
  "https://news.detik.com/berita/d-6570699/apa-perbedaan-tuntutan-dan-vonis-simak-penjelasannya" \
  "Tuntutan adalah wewenang yang dimiliki oleh penuntut umum atau Jaksa Penuntut Umum (JPU)." \
  --hide "$DH" \
  -o assets/pages/detik-beda-tuntutan-vonis-tuntutan.png

[ -s assets/pages/detik-beda-tuntutan-vonis-vonis.png ] || node "$PQ" \
  "https://news.detik.com/berita/d-6570699/apa-perbedaan-tuntutan-dan-vonis-simak-penjelasannya" \
  "Vonis disebut juga putusan hakim terhadap terdakwa dalam perkara pidana." \
  --hide "$DH" \
  -o assets/pages/detik-beda-tuntutan-vonis-vonis.png

# Ibam (Ibrahim Arief): demand and verdict
[ -s assets/pages/kompas-ibam-dituntut-15-tahun.png ] || node "$PQ" \
  "https://nasional.kompas.com/read/2026/04/16/19414591/eks-konsultan-teknologi-ibrahim-arief-dituntut-15-tahun-penjara-kasus" \
  "Ibrahim Arief dituntut 15 tahun penjara" \
  --hide "$KH" \
  -o assets/pages/kompas-ibam-dituntut-15-tahun.png

[ -s assets/pages/kompas-ibam-tambahan-7-5-tahun.png ] || node "$PQ" \
  "https://nasional.kompas.com/read/2026/04/16/19414591/eks-konsultan-teknologi-ibrahim-arief-dituntut-15-tahun-penjara-kasus" \
  "JPU menuntut agar Ibrahim dihukum penjara tambahan 7,5 tahun." \
  --hide "$KH" \
  -o assets/pages/kompas-ibam-tambahan-7-5-tahun.png

[ -s assets/pages/cnn-ibam-divonis-4-tahun.png ] || node "$PQ" \
  "https://www.cnnindonesia.com/nasional/20260512120239-12-1357783/ibam-konsultan-proyek-chromebook-divonis-4-tahun-penjara" \
  "dengan pidana penjara selama 4 tahun" \
  --also "Ibam dituntut dengan pidana 15 tahun penjara" \
  -o assets/pages/cnn-ibam-divonis-4-tahun.png

# The named commentators: Mahfud MD, Hotman Paris, Ferry Irwandi
[ -s assets/pages/kompas-mahfud-tetap-korupsi.png ] || node "$PQ" \
  "https://nasional.kompas.com/read/2026/01/14/18255991/nadiem-ngaku-tak-terima-uang-di-kasus-chromebook-mahfud-md-tetap-korupsi" \
  "Itu sudah korupsi, kalau merugikan keuangan negara." \
  --hide "$KH" \
  -o assets/pages/kompas-mahfud-tetap-korupsi.png

[ -s assets/pages/detik-hotman-buktikan-tidak-korupsi.png ] || node "$PQ" \
  "https://www.detik.com/bali/hukum-dan-kriminal/d-8098713/hotman-paris-bela-nadiem-sentil-prabowo-soal-kasus-korupsi-laptop" \
  "saya akan membuktikan bahwa Nadiem Makarim tidak melakukan tindak pidana korupsi" \
  --hide "$DH" \
  -o assets/pages/detik-hotman-buktikan-tidak-korupsi.png

[ -s assets/pages/kompastv-hotman-jangan-pelit.png ] || node "$PQ" \
  "https://www.kompas.tv/regional/669631/hotman-paris-ke-nadiem-makanya-sama-pengacara-itu-tidak-boleh-pelit-ya" \
  "makanya ya sama pengacara itu harus benar-benar tidak boleh pelit ya" \
  -o assets/pages/kompastv-hotman-jangan-pelit.png

[ -s assets/pages/cnn-hotman-jangan-pelit.png ] || node "$PQ" \
  "https://www.cnnindonesia.com/nasional/20260518135941-12-1359589/hotman-paris-sentil-nadiem-makanya-jangan-pelit-sama-pengacara" \
  "makanya lain kali jangan pelit-pelit sama pengacara" \
  -o assets/pages/cnn-hotman-jangan-pelit.png

[ -s assets/pages/suara-ferry-irwandi-narasi.png ] || node "$PQ" \
  "https://www.suara.com/news/2026/04/28/130921/sebut-narasi-ferry-irwandi-sesat-pikir-di-kasus-chromebook-pengamat-hukum-itu-fakta-bukan-opini" \
  "Pernyataan konten kreator Ferry Irwandi yang membela Ibam" \
  --hide ".wrap-ads,.widget-ads3,.widget-ads4,.sticky-ads,.banner-sidebar-custom,.bottom-ads,iframe" \
  -o assets/pages/suara-ferry-irwandi-narasi.png

# Public reaction, and the book
[ -s assets/pages/goodstats-sentimen-warganet.png ] || node "$PQ" \
  "https://data.goodstats.id/statistic/nadiem-makarim-dituntut-18-tahun-penjara-bagaimana-respons-warganet-azZkS" \
  "Banyak warganet menilai Nadiem menjadi korban kriminalisasi dan tebang pilih hukum." \
  --also "sentimen negatif tercatat sebesar 17,4%." \
  --also "didominasi sentimen positif dengan persentase mencapai 78,7%." \
  --height 760 \
  -o assets/pages/goodstats-sentimen-warganet.png

[ -s assets/pages/wikipedia-death-of-expertise.png ] || node "$PQ" \
  "https://en.wikipedia.org/wiki/The_Death_of_Expertise" \
  "Nichols condemns what he describes as the many forces trying to undermine the authority of experts in the United States." \
  -o assets/pages/wikipedia-death-of-expertise.png

# Football: the VAR handball, and pundits who disagree
[ -s assets/pages/skysports-handball-var.png ] || node "$PQ" \
  "https://www.skysports.com/football/news/11661/13545159/man-utd-3-2-nottingham-forest-vitor-pereira-calls-for-premier-league-meeting-to-clarify-rules-after-very-clear-handball" \
  "The VAR has been clear: the player has handballed it." \
  --height 560 \
  --hide ".sdc-article-video,.sdc-site-video,.sdc-article-widget,video,iframe,[data-component-name='sdc-article-video'],.sdc-site-outbrain" \
  -o assets/pages/skysports-handball-var.png

[ -s assets/pages/skysports-handball-var-top.png ] || node "$PQ" \
  "https://www.skysports.com/football/news/11661/13545159/man-utd-3-2-nottingham-forest-vitor-pereira-calls-for-premier-league-meeting-to-clarify-rules-after-very-clear-handball" \
  "Vitor Pereira calls for Premier League meeting to clarify rules after 'very clear' handball" \
  --top --height 700 \
  -o assets/pages/skysports-handball-var-top.png

[ -s assets/pages/planetfootball-neville-carragher-debate.png ] || node "$PQ" \
  "https://www.planetfootball.com/videos/watch-carragher-hits-head-on-table-during-paul-pogba-debate-with-neville" \
  "but Carragher disagreed" \
  --height 400 --hide "iframe,.twitter-tweet,[class*=ad-],[id*=ad-]" \
  -o assets/pages/planetfootball-neville-carragher-debate.png

# ---------------------------------------------------------------- third-party clips

# A section of a YouTube video into source/clips/<name>.webm. yt-dlp starts the section on the
# keyframe before the start asked for, so the file is longer than the range, and the in-points
# of the conform below are measured in THIS file (VP9 in WebM, as YouTube served it on 2026-10-09).
section() {  # <name> <start-end> <video id>
  [ -s "source/clips/$1.webm" ] || yt-dlp -q --no-warnings --download-sections "*$2" \
    -f "bv*[height<=1080]+ba/b[height<=1080]" -o "source/clips/$1.%(ext)s" -- "$3"
  [ -s "source/clips/$1.webm" ] || {
    echo "make-evidence: source/clips/$1.webm was not made (YouTube served another format, or refused): the in-point of $1 must be found again" >&2
    exit 1
  }
}

# The conform: 1920x1080 (scaled and cropped to fill), 30 fps, H.264 yuv420p with a keyframe every
# second, AAC 48 kHz stereo, a 50 ms audio fade in and a 60 ms fade out that ends with the clip.
conform() {  # <name> <in-point in the section, s> <length, s> <fade-out start = length - 0.06>
  [ -s "footage/$1.mp4" ] || ffmpeg -v error -y -ss "$2" -i "source/clips/$1.webm" -t "$3" \
    -vf "scale=1920:1080:force_original_aspect_ratio=increase,crop=1920:1080,fps=30,format=yuv420p" \
    -af "afade=t=in:st=0:d=0.05,afade=t=out:st=$4:d=0.06" \
    -c:v libx264 -preset medium -crf 18 -pix_fmt yuv420p -g 30 -keyint_min 30 -sc_threshold 0 \
    -c:a aac -ar 48000 -ac 2 -b:a 192k -movflags +faststart "footage/$1.mp4"
}

# c01: the prosecutor reads the demand (KOMPASTV, KOMPAS PETANG, 16 May 2026)
section c01 "00:16-00:30" Kxg5C6dxuus
conform c01 9.2 8.15 8.09

# c02: Nadiem on the replacement money (same video)
section c02 "01:15-01:27" Kxg5C6dxuus
conform c02 9.65 5.2 5.14

# c03: Nadiem compares the demand with murderers and terrorists (same video)
section c03 "01:23-01:35" Kxg5C6dxuus
conform c03 5.45 7.3 7.24

# c04: Hotman Paris, "tidak boleh pelit" (KOMPASTV, 18 May 2026)
section c04 "01:45-01:57" 8Q93ufimaV8
conform c04 7.5 6.95 6.89

# c05: Mahfud MD on what counts as corruption (METRO TV, Primetime News, 7 January 2026)
section c05 "01:16-01:30" 7PZK2Ko83jY
conform c05 9.3 7.35 7.29

# c06: Gary Neville disagrees with Jamie Carragher (Sky Sports Premier League, 1 September 2024)
section c06 "04:08-04:24" P7Chg8np8sY
conform c06 11.5 5.55 5.49

echo "make-evidence: done ($(ls assets/pages/*.png | wc -l | tr -d ' ') pages, $(ls footage/c0[1-6].mp4 | wc -l | tr -d ' ') clips)"

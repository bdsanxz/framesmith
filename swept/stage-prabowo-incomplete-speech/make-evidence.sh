#!/usr/bin/env bash
# Makes the evidence of this film again from nothing: the 28 page captures in assets/pages/
# (with their .json boxes) and the 12 quoted clips footage/c01.mp4 to c12.mp4 (raw sections in
# source/clips/). A step is skipped when its output file already exists. Web pages change (text,
# layout, ads) and YouTube may serve other encodings, so a capture or cut made later may differ
# from the one made on 2026-10-09: look at every new picture and clip before using it.
set -euo pipefail
cd "$(dirname "$0")"

mkdir -p assets/pages source/clips footage

# pq <out.png> <url> <passage> [page-quote options...]
pq() {
  local out=$1 url=$2 passage=$3
  shift 3
  [ -s "$out" ] || node ../../scripts/page-quote.mjs "$url" "$passage" -o "$out" "$@"
}

# dl <name> <youtube url> <HH:MM:SS-HH:MM:SS>: the raw section, source/clips/<name>.mp4
dl() {
  [ -s "source/clips/$1.mp4" ] || yt-dlp -q --no-warnings --download-sections "*$3" \
    -f "bv*[height<=1080]+ba/b[height<=1080]" --merge-output-format mp4 \
    -o "source/clips/$1.%(ext)s" "$2"
}

# conform <out name> <raw name> <start s> <end s>: footage/<out>.mp4, cut from source/clips/<raw>.mp4
conform() {
  local out=$1 src=$2 a=$3 b=$4 d fo
  [ -s "footage/$out.mp4" ] && return 0
  d=$(awk -v a="$a" -v b="$b" 'BEGIN { printf "%.3f", b - a }')
  fo=$(awk -v a="$a" -v b="$b" 'BEGIN { printf "%.3f", b - a - 0.06 }')
  ffmpeg -v error -y -ss "$a" -t "$d" -i "source/clips/$src.mp4" \
    -vf "scale=1920:1080:force_original_aspect_ratio=increase,crop=1920:1080,fps=30,format=yuv420p" \
    -af "afade=t=in:d=0.03,afade=t=out:st=$fo:d=0.06" \
    -c:v libx264 -preset medium -crf 17 -pix_fmt yuv420p -g 30 -keyint_min 30 -sc_threshold 0 \
    -c:a aac -b:a 192k -ar 48000 -ac 2 -movflags +faststart "footage/$out.mp4.part.mp4"
  mv "footage/$out.mp4.part.mp4" "footage/$out.mp4"
}

# ---------------------------------------------------------------------------------------------
# Page captures
# ---------------------------------------------------------------------------------------------
ANTARA_TIBA="https://www.antaranews.com/berita/5574196/prabowo-tiba-di-gedung-nusantara-untuk-pidato-kem-ppkf-rapbn-2027"
ANTARA_APBN="https://www.antaranews.com/berita/5574300/presiden-apbn-alat-lindungi-dan-pastikan-kesejahteraan-rakyat"
ANTARA_KAGUM="https://www.antaranews.com/berita/5574313/prabowo-jangan-kagum-pada-negara-yang-kaya-karena-rampas-bangsa-lain"
SETKAB_TARGETS="https://setkab.go.id/presiden-prabowo-paparkan-arah-kebijakan-ekonomi-nasional-melalui-kem-ppkf-2027-pertumbuhan-ekonomi-diproyeksi-capai-65-persen/"
ADS="iframe,ins,[id*=google_ads],[id*=div-gpt],[class*=ads]"
ADS_KONTAN="iframe,ins,[id*=google_ads],[id*=div-gpt],[class*=ads],[class*=banner],[class*=iklan]"
BREADCRUMB=".breadcrumb,[class*=breadcrumb]"

pq assets/pages/p01-antara-first-time.png "$ANTARA_TIBA" \
  "Untuk pertama kalinya, Presiden Prabowo akan berpidato menyampaikan secara langsung kebijakan ekonomi makro"

pq assets/pages/p01-antara-first-time-top.png "$ANTARA_TIBA" \
  "Prabowo tiba di Gedung Nusantara untuk pidato KEM-PPKF RAPBN 2027" \
  --top --hide "$BREADCRUMB"

pq assets/pages/p02-kompastv-dasco-first.png \
  "https://www.kompas.tv/nasional/669802/prabowo-subianto-akan-berpidato-soal-ekonomi-makro-di-rapat-paripurna-dpr-dasco-ini-pertama-kali" \
  "mungkin ini pertama kalinya seorang Presiden RI berpidato soal Kerangka Ekonomi Makro"

pq assets/pages/p03-setkab-why-in-person.png \
  "https://setkab.go.id/presiden-prabowo-sampaikan-kerangka-ekonomi-makro-dan-pokok-kebijakan-fiskal-rapbn-2027-di-sidang-paripurna-dpr-ri/" \
  "saya berpendapat bahwa Presiden Republik Indonesia harus hadir langsung menyampaikan pokok-pokok pikiran perekonomian dan pengelolaan negara" \
  --also "tantangan geopolitik dan geoekonomi dunia yang penuh ketegangan dan ketidakpastian"

pq assets/pages/p04-antara-apbn-alat-perjuangan.png "$ANTARA_APBN" \
  "APBN adalah wujud dari alat perjuangan kita sebagai bangsa" \
  --also "APBN adalah alat untuk melindungi rakyat"

pq assets/pages/p05-setkab-targets-rupiah-sbn-inflation.png "$SETKAB_TARGETS" \
  "Nilai tukar rupiah terhadap dolar Amerika Serikat berada pada rentang 16.800 rupiah hingga 17.500 rupiah" \
  --also "Suku bunga surat berharga negara (SBN) tenor 10 tahun dijaga pada kisaran 6,5 hingga 7,3 persen" \
  --also "Inflasi akan kami jaga tetap pada kisaran 1,5 hingga 3,5 persen"

pq assets/pages/p05-setkab-targets-top.png "$SETKAB_TARGETS" \
  "Presiden Prabowo Paparkan Arah Kebijakan Ekonomi Nasional Melalui KEM PPKF 2027, Pertumbuhan Ekonomi Diproyeksi Capai 6,5 Persen" \
  --top --keep-fixed

pq assets/pages/p06-setkab-targets-growth-poverty-unemployment.png "$SETKAB_TARGETS" \
  "ekonomi Indonesia dapat tumbuh pada kisaran 5,8 hingga 6,5 persen di tahun 2027" \
  --also "angka kemiskinan ditargetkan turun ke rentang 6,0 hingga 6,5 persen" \
  --also "tingkat pengangguran terbuka harus turun pada rentang 4,30 hingga 4,87 persen" \
  --also "Rasio gini ditargetkan semakin membaik pada rentang 0,362 hingga 0,367" \
  --height 700

pq assets/pages/p07-setkab-formal-jobs.png "$SETKAB_TARGETS" \
  "proporsi lapangan kerja formal meningkat menjadi 40,81 persen di 2027"

pq assets/pages/p08-kontan-rupiah-record-20may.png \
  "https://pusatdata.kontan.co.id/news/rupiah-melemah-hari-ini-225-rekor-terburuk-sepanjang-masa-terulang-lagi" \
  "paling lemah rupiah sepanjang masa berada di Rp 17.748 per dolar AS yang tercatat pada 20 Mei" \
  --width 1240 --hide "$ADS_KONTAN"

pq assets/pages/p09-kontan-rupiah-26may.png \
  "https://investasi.kontan.co.id/news/rupiah-ambruk-ke-rekor-penutupan-terlemah-rp-17796-per-dolar-as-selasa-2652026" \
  "rupiah di pasar spot ditutup di level Rp 17.796 per dolar AS"

pq assets/pages/p10-inews-below-dogs.png \
  "https://www.inews.id/news/nasional/prabowo-pidato-di-dpr-bangsa-indonesia-pernah-ditempatkan-di-bawah-anjing-oleh-penjajah/2" \
  "Mereka merasakan bahwa bangsa Indonesia ditempatkan derajatnya di bawah anjing" \
  --also "kekayaan Nusantara ratusan tahun diambil oleh penjajah"

pq assets/pages/p11-antara-do-not-admire.png "$ANTARA_KAGUM" \
  "Hendaknya janganlah kita terlalu kagum kepada bangsa-bangsa yang kayanya dari merampas kekayaan bangsa-bangsa lain"

pq assets/pages/p11-antara-do-not-admire-top.png "$ANTARA_KAGUM" \
  "Prabowo: Jangan kagum pada negara yang kaya karena rampas bangsa lain" \
  --top --hide "$BREADCRUMB"

pq assets/pages/p12-antara-learn-from-history.png "$ANTARA_KAGUM" \
  "mereka yang tidak belajar dari sejarah akan dihukum oleh sejarah"

pq assets/pages/p13-antara-netherlands-gdp.png "$ANTARA_KAGUM" \
  "per kapita tertinggi di dunia selama beberapa abad sejak tahun 1500-an hingga 1800-an"

pq assets/pages/p14-investortrust-netherlands-400-years.png \
  "https://investortrust.id/national/103840/prabowo-ungkap-pdb-belanda-tertinggi-di-dunia-selama-400-tahun-karena-kuasai-nusantara" \
  "400 tahun telah memiliki PDB per kapita tertinggi"

pq assets/pages/p15-detik-prabowo-menjawab-2025.png \
  "https://news.detik.com/berita/d-7858543/prabowo-diwawancarai-7-jurnalis-senior-golkar-presiden-terbuka-tanpa-sensor" \
  "Prabowo diwawancarai selama empat jam tanpa mengetahui daftar pertanyaan yang akan diajukan"

pq assets/pages/p16-cnn-hambalang-2026.png \
  "https://www.cnnindonesia.com/nasional/20260319135338-32-1339642/prabowo-bertemu-chatib-basri-hingga-najwa-shihab-di-hambalang" \
  "Diskusi yang berlangsung pada pukul 19.00-01.30 WIB pada tanggal 17 Maret 2026" \
  --hide "video,iframe,ins,[class*=ads],[id*=ads],[class*=video],[id*=video],[class*=parallax],[class*=banner],[class*=inread],[class*=inRead],[id*=inread],[class*=teads],[class*=outstream],[class*=player],[id*=player],[class*=sticky],[style*='background: rgb(0, 0, 0)'],[style*='background-color: rgb(0, 0, 0)'],[style*='background:#000'],[style*='background-color:#000'],[style*='background: black']"

pq assets/pages/p17-kompas-trump-document.png \
  "https://video.kompas.com/watch/1911552/momen-prabowo-diminta-pegang-dokumen-board-of-peace-oleh-trump" \
  "meminta Presiden Prabowo Subianto untuk memegang dokumen Board of Peace"

pq assets/pages/p18-suara-trump-document.png \
  "https://www.suara.com/tekno/2026/02/24/144537/viral-donald-trump-minta-prabowo-pegangi-dokumen-bop-dan-pulpen-ekspresinya-jadi-sorotan" \
  "Sebuah momen saat Presiden AS Donald Trump meminta Prabowo memegangi dokumen Board of Peace (BoP) viral menuai sorotan" \
  --wait 5000 --hide "$ADS"

pq assets/pages/p19-setkab-article-33.png \
  "https://setkab.go.id/di-sidang-paripurna-dpr-presiden-prabowo-tegaskan-komitmen-jalankan-amanat-uud-1945/" \
  "Kepala Negara menilai Pasal 33 merupakan cetak biru perekonomian nasional" \
  --also "Perekonomian disusun sebagai usaha bersama berdasar atas asas kekeluargaan"

pq assets/pages/p20-goodstats-foreign-trips.png \
  "https://goodstats.id/article/1-5-tahun-memimpin-prabowo-sudah-kunjungi-56-negara-dan-forum-internasional-rekor-baru-diplomasi-indonesia-aSwTl" \
  "Prabowo tercatat telah melakukan 56 kunjungan luar negeri" \
  --wait 6000 --hide "[role=dialog],[class*=modal],[class*=Modal],[class*=popup],[class*=overlay],[class*=backdrop],[data-state=open],[class*=bg-black]"

pq assets/pages/p21-setkab-investors-washington.png \
  "https://setkab.go.id/presiden-prabowo-terima-12-ceo-global-di-washington-dc-perkuat-kemitraan-investasi-strategis/" \
  "menerima 12 pimpinan perusahaan investasi global dalam sebuah pertemuan sebagai bagian dari kunjungan kerjanya di Washington DC" \
  --hide "figcaption,[class*=caption]"

pq assets/pages/p22-maddison-project-database.png \
  "https://www.rug.nl/ggdc/historicaldevelopment/maddison/releases/maddison-project-database-2020" \
  "information on comparative economic growth and income levels over the very long run"

pq assets/pages/p23-investortrust-2026-growth-target.png \
  "https://investortrust.id/macro/79864/tok-dpr-sepakat-asumsi-makro-indikator-kesejahteraan-dan-postur-apbn-2026-jadi-undang-undang" \
  "Target pertumbuhan ekonomi 2026 ditetapkan 5,4%"

pq assets/pages/p24-metrotv-targets-rupiah.png \
  "https://www.metrotvnews.com/read/bmRCY1w9-ini-target-rapbn-dan-asumsi-ekonomi-makro-untuk-2027" \
  "dalam rentang Rp16.800 hingga Rp17.500 per dolar AS" \
  --wait 5000 --hide "$ADS"

pq assets/pages/p25-kontan-rupiah-close-20may.png \
  "https://investasi.kontan.co.id/news/bi-kerek-suku-bunga-rupiah-ditutup-menguat-ke-rp-17654-per-dolar-as-hari-ini-205" \
  "Rabu (20/5/2026), rupiah ditutup di level Rp 17.654 per dolar Amerika Serikat (AS)" \
  --width 1240 --hide "$ADS_KONTAN"

# ---------------------------------------------------------------------------------------------
# Clips. The raw section starts on a keyframe near the time asked for, so the cut times below
# (seconds inside the raw file) hold only for the raw files this download gave on 2026-10-09:
# after a new download, transcribe the clip and check its first and last frame.
# ---------------------------------------------------------------------------------------------
SPEECH="https://www.youtube.com/watch?v=P9VLfvhctUE"    # Sekretariat Presiden, the speech, 20 May 2026
MENJAWAB="https://www.youtube.com/watch?v=okceXc5LLU8"  # Prabowo Subianto, "Presiden Prabowo Menjawab !!! (Bagian 2)"
BOP="https://www.youtube.com/watch?v=fqsamtwaprM"       # Kompas.com, Trump hands Prabowo the Board of Peace folder

dl c01 "$SPEECH" 00:58:44-00:59:05
dl c02 "$SPEECH" 00:58:00-00:58:25
dl c03 "$SPEECH" 00:59:50-01:00:18
dl c04 "$SPEECH" 01:09:17-01:09:34
dl c05 "$SPEECH" 01:09:05-01:09:21
dl c06 "$SPEECH" 01:09:30-01:09:50
dl c07 "$SPEECH" 01:11:22-01:12:47   # one raw file for c07, c08 and c09
dl c10 "$SPEECH" 01:58:28-01:59:18
dl c11 "$MENJAWAB" 00:02:24-00:02:48
dl c12 "$BOP" 00:00:38-00:01:06

conform c01 c01 8.92 13.65    # "Saya berpendapat bahwa Presiden Republik Indonesia harus hadir langsung"
conform c02 c02 11.05 21.50   # "geopolitik dan geoekonomi yang penuh dengan konflik, ketegangan, dan ketidakpastian"
conform c03 c03 13.05 18.90   # "APBN adalah wujud dari alat perjuangan kita sebagai bangsa"
conform c04 c04 4.15 13.85    # "Mereka melihat dan merasakan kekayaan Nusantara ratusan tahun diambil ..."
conform c05 c05 5.65 12.45    # "... ditempatkan derajatnya di bawah anjing"
conform c06 c06 5.28 15.35    # "Janganlah kita terlalu kagum ..."
conform c07 c07 40.75 46.70   # "400 tahun telah memiliki PDB per kapita tertinggi"
conform c08 c07 63.60 72.45   # "bisa memiliki PDB tertinggi di dunia. Kenapa? Karena mereka menguasai Nusantara kita"
conform c09 c07 77.20 83.50   # the GDP-per-capita chart on screen
conform c10 c10 13.40 19.72   # "Kau masuk keluar kan tiap lima tahun ... kalau enggak di-reshuffle"
conform c11 c11 11.62 19.88   # the round table at Hambalang, wide shot
conform c12 c12 11.50 18.00   # Trump hands the folder to Prabowo

set term png size 1200,800;
set output fileout;
set ylabel "KBytes";
set xlabel "Block Height";
#set key off;

set title "RocksDB size growth";
plot filein using 1:2 with lines axis x1y1 title "DB Size"

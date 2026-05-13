# 各種コマンド
#FSLC	 = fslc
FSLC	 = nfc
IVERILOG = iverilog
VVP	 = vvp

# quartus各種コマンド
QUARTUS_MAP = quartus_map
QUARTUS_FIT = quartus_fit
QUARTUS_STA = quartus_sta
# quartusコマンドのオプション
QMAP_OPTS = --family="Cyclone IV E"
QFIT_OPTS = --part=EP4CE115F29C7
QSTA_OPTS = --do_report_timing


# FSL記述を増やしたら，下記の行に追加する
SRCS 	= alu32.fsl add32.fsl test_alu32.fsl
# テストベンチのターゲットモジュール
TARGET	= test_alu32
# 合成時のトップモジュール
TOP     = top_alu32
# シミュレーション結果を保存するファイル
TEMPFILES = test_alu32.result
# テストベクタ（入力信号パターン，結果比較用）
TESTVECTORS = test_alu32.pat test_alu32.result.sample

# make だけのときは，-verilog オプション等なしコンパイル
all:	compile

# シミュレーションするとき make sim
sim:	test_alu32.result

# .vvpファイルを生成するときは make vvp
vvp:	$(TARGET).vvp

# コンパイル（エラーチェックのみ）
compile: $(SRCS)
	$(FSLC) $^ 

# Verilog HDLファイルをつくるときは，make v あるいは make verilog
v:	verilog
#verilog: $(SRCS:.fsl=.v)
verilog: $(TOP).fsl $(SRCS)
	$(FSLC) $^ -verilog -target $(TARGET)

# Verilog HDL ファイルの生成（コンパイル）
$(SRCS:.fsl=.v): $(TOP).fsl $(SRCS)
	$(FSLC) $^ -verilog -target $(TARGET)

# .vvp ファイル（シミュレーションスクリプト）の生成
test_alu32.vvp: $(SRCS:.fsl=.v)
	$(IVERILOG) -o $@ $^

# .vvpファイルを実行（シミュレーション）して，結果をファイルに保存
test_alu32.result: $(TARGET).vvp
	$(VVP) $(TARGET).vvp | tee $@
	@echo "result file '$@' generated"

# シミュレーション結果と比較用パターンとの差分をとる
diff: test_alu32.result test_alu32.result.sample
	diff test_alu32.result test_alu32.result.sample || exit $$(($$? - 1))

# FPGA map
map:    $(SRCS:.fsl=.v) $(TOP).v
	$(QUARTUS_MAP) $(QMAP_OPTS) $(TOP).v
	@cat $(TOP).map.summary

# FPGA fit
fit:    $(SRCS:.fsl=.v) $(TOP).v
	$(QUARTUS_FIT) $(QFIT_OPTS) $(TOP)
	@cat $(TOP).fit.summary

# FPGA sta
sta:    $(SRCS:.fsl=.v) $(TOP).v
	$(QUARTUS_STA) $(QSTA_OPTS) $(TOP)
	@if [ -f $(TOP).sta.rpt ]; then \
	  grep -B 3 -A 3 "; Fmax" $(TOP).sta.rpt; \
	fi
fmax:
	@if [ -f $(TOP).sta.rpt ]; then \
	  grep -B 3 -A 3 "; Fmax" $(TOP).sta.rpt; \
	else \
	  echo "No TimeQuest Timing Analyzer report ($(TOP).sta.rpt)."; \
	fi

clean:
	rm -f *~ $(TEMPFILES) 
	rm -f $(SRCS:.fsl=.v)
	rm -f *.vcd

distclean: clean
	rm -f $(TARGET).vvp

pull:
	git pull

# End of file (Makefile)

